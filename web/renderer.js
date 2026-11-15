// WebGPU 渲染器：阴影贴图 → 相机深度 → SSAO + 模糊 → 主 PBR 通道。
// Z-up 世界坐标；爆炸/拆装位移完全在着色器内完成，动画不触发缓冲重建。

import { buildColorTextures } from './textures.js';

const SHADOW_SIZE = 1024;

export class DougongRenderer {
  constructor(canvas) {
    this.canvas = canvas;
    this.device = null;
    this.context = null;
    this.buffers = null;
    this.dynamic = { explodeScale: 0, disassemble: 0, maxLayer: 1, time: 0 };
    this.camera = { yaw: 0.7, pitch: 0.55, dist: 2.2, target: [0, 0, 0.35] };
    this.style = 'wood';
    this.textureSet = null;
    this.frame = 0;
  }

  async init() {
    if (!navigator.gpu) throw new Error('当前浏览器不支持 WebGPU');
    const adapter = await navigator.gpu.requestAdapter({ powerPreference: 'high-performance' });
    if (!adapter) throw new Error('找不到可用的 GPU 适配器');
    this.device = await adapter.requestDevice();
    this.device.lost.then((info) => console.error('GPU device lost:', info));

    const context = this.canvas.getContext('webgpu');
    // 画布用非 srgb 格式，shader 内手动做线性→sRGB 编码（见 fs_main 末尾）。
    const format = 'bgra8unorm';
    context.configure({ device: this.device, format, alphaMode: 'opaque' });
    this.context = context;
    this.format = format;

    await this.createPipelines();
    this.bindControls();
    const loop = () => {
      this.render();
      this.raf = requestAnimationFrame(loop);
    };
    this.raf = requestAnimationFrame(loop);
  }

  async createPipelines() {
    const device = this.device;
    const shader = await fetch('./shaders.wgsl').then((r) => r.text());
    this.module = device.createShaderModule({ code: shader });

    // 顶点缓冲：position(3) normal(3) matId(u32) compId(u32) layer(u32) explodeDir(3)，
    // 步距 12 个 float。
    this.vertexBufferLayout = {
      arrayStride: 48,
      attributes: [
        { shaderLocation: 0, offset: 0, format: 'float32x3' },
        { shaderLocation: 1, offset: 12, format: 'float32x3' },
        { shaderLocation: 2, offset: 24, format: 'uint32' },
        { shaderLocation: 3, offset: 28, format: 'uint32' },
        { shaderLocation: 4, offset: 32, format: 'uint32' },
        { shaderLocation: 5, offset: 36, format: 'float32x3' },
      ],
    };

    this.cameraUniform = device.createBuffer({ size: 160, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
    this.dynamicUniform = device.createBuffer({ size: 16, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
    this.aoUniform = device.createBuffer({ size: 144, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
    this.materialUniform = device.createBuffer({ size: 128, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
    this.screenUniform = device.createBuffer({ size: 16, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });

    const sceneLayout = device.createBindGroupLayout({
      entries: [
        { binding: 0, visibility: GPUShaderStage.VERTEX | GPUShaderStage.FRAGMENT, buffer: { type: 'uniform' } },
        { binding: 1, visibility: GPUShaderStage.VERTEX, buffer: { type: 'uniform' } },
        { binding: 2, visibility: GPUShaderStage.FRAGMENT, buffer: { type: 'uniform' } },
      ],
    });
    this.sceneLayout = sceneLayout;
    this.texLayout = device.createBindGroupLayout({
      entries: [
        { binding: 0, visibility: GPUShaderStage.FRAGMENT, texture: {} },
        { binding: 1, visibility: GPUShaderStage.FRAGMENT, sampler: {} },
      ],
    });
    this.postLayout = device.createBindGroupLayout({
      entries: [
        { binding: 0, visibility: GPUShaderStage.FRAGMENT, texture: { sampleType: 'depth' } },
        { binding: 1, visibility: GPUShaderStage.FRAGMENT, texture: {} },
        { binding: 2, visibility: GPUShaderStage.FRAGMENT, sampler: {} },
        { binding: 3, visibility: GPUShaderStage.FRAGMENT, buffer: { type: 'uniform' } },
        { binding: 4, visibility: GPUShaderStage.FRAGMENT, buffer: { type: 'uniform' } },
        { binding: 5, visibility: GPUShaderStage.FRAGMENT, texture: { sampleType: 'depth' } },
      ],
    });
    this.matLayout = device.createBindGroupLayout({
      entries: [{ binding: 0, visibility: GPUShaderStage.FRAGMENT, buffer: { type: 'uniform' } }],
    });
    this.aoTexLayout = device.createBindGroupLayout({
      entries: [
        { binding: 0, visibility: GPUShaderStage.FRAGMENT, texture: {} },
        { binding: 1, visibility: GPUShaderStage.FRAGMENT, sampler: {} },
        { binding: 2, visibility: GPUShaderStage.FRAGMENT, buffer: { type: 'uniform' } },
      ],
    });

    this.pipelineLayout = device.createPipelineLayout({
      bindGroupLayouts: [sceneLayout, this.texLayout, this.postLayout, this.matLayout],
    });
    this.depthLayout = device.createPipelineLayout({ bindGroupLayouts: [sceneLayout] });
    this.ssaoLayout = device.createPipelineLayout({
      bindGroupLayouts: [sceneLayout, this.texLayout, this.postLayout, this.matLayout],
    });
    this.blurLayout = device.createPipelineLayout({
      bindGroupLayouts: [this.aoTexLayout],
    });

    const depthState = {
      format: 'depth32float',
      depthWriteEnabled: true,
      depthCompare: 'less',
    };
    // 各图元生成路径的三角形绕向不完全统一，关闭剔除靠 shader 双面光照保证正确。
    const prim = { topology: 'triangle-list', cullMode: 'none' };

    this.shadowPipeline = device.createRenderPipeline({
      layout: this.depthLayout,
      vertex: { module: this.module, entryPoint: 'vs_shadow', buffers: [this.vertexBufferLayout] },
      primitive: prim,
      depthStencil: depthState,
    });
    this.depthPipeline = device.createRenderPipeline({
      layout: this.depthLayout,
      vertex: { module: this.module, entryPoint: 'vs_depth', buffers: [this.vertexBufferLayout] },
      primitive: prim,
      depthStencil: depthState,
    });
    this.mainPipeline = device.createRenderPipeline({
      layout: this.pipelineLayout,
      vertex: { module: this.module, entryPoint: 'vs_main', buffers: [this.vertexBufferLayout] },
      fragment: {
        module: this.module,
        entryPoint: 'fs_main',
        targets: [{ format: this.format }],
      },
      primitive: prim,
      depthStencil: depthState,
    });
    this.ssaoPipeline = device.createRenderPipeline({
      layout: this.ssaoLayout,
      vertex: { module: this.module, entryPoint: 'vs_fullscreen' },
      fragment: { module: this.module, entryPoint: 'fs_ssao', targets: [{ format: 'r8unorm' }] },
      primitive: { topology: 'triangle-list' },
    });
    this.blurPipeline = device.createRenderPipeline({
      layout: this.blurLayout,
      vertex: { module: this.module, entryPoint: 'vs_fullscreen' },
      fragment: { module: this.module, entryPoint: 'fs_blur', targets: [{ format: 'r8unorm' }] },
      primitive: { topology: 'triangle-list' },
    });
  }

  async setModel(gpuData, style) {
    const device = this.device;
    this.gpuData = gpuData;
    this.style = style;

    // 交错顶点缓冲（位置/法线/材质/构件/层/爆炸方向）。
    const vcount = gpuData.positions.length / 3;
    const interleaved = new Float32Array(vcount * 12);
    for (let i = 0; i < vcount; i++) {
      interleaved.set(gpuData.positions.subarray(i * 3, i * 3 + 3), i * 12);
      interleaved.set(gpuData.normals.subarray(i * 3, i * 3 + 3), i * 12 + 3);
      interleaved[i * 12 + 6] = gpuData.materials[i];
      interleaved[i * 12 + 7] = gpuData.compIds[i];
      interleaved[i * 12 + 8] = gpuData.layers[i];
      interleaved.set(gpuData.explodeDirs.subarray(i * 3, i * 3 + 3), i * 12 + 9);
    }
    this.vertexBuffer = device.createBuffer({
      size: interleaved.byteLength,
      usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST,
    });
    device.queue.writeBuffer(this.vertexBuffer, 0, interleaved);

    this.indexBuffer = device.createBuffer({
      size: gpuData.indices.byteLength,
      usage: GPUBufferUsage.INDEX | GPUBufferUsage.COPY_DST,
    });
    device.queue.writeBuffer(this.indexBuffer, 0, gpuData.indices);
    this.indexCount = gpuData.indices.length;

    // 材质 uniform：xyz 基色（线性），w 粗糙度。
    const matData = new Float32Array(32);
    gpuData.materialsDef.forEach((m, i) => {
      matData.set(m.baseColor, i * 4);
      matData[i * 4 + 3] = 0.72;
    });
    device.queue.writeBuffer(this.materialUniform, 0, matData);

    // 程序化纹理（按当前风格）。
    if (this.textureSet) {
      this.textureSet.views.forEach((v) => { /* GPU 资源随 GC 回收 */ });
    }
    this.textureSet = await buildColorTextures(device, gpuData.materialsDef, style);
    this.materialRanges = this.buildMaterialRanges(gpuData);

    this.createFrameResources();
    this.frameCamera(gpuData.bounds);
  }

  createFrameResources() {
    const device = this.device;
    const [w, h] = [this.canvas.width, this.canvas.height];

    this.shadowTexture = device.createTexture({
      size: [SHADOW_SIZE, SHADOW_SIZE],
      format: 'depth32float',
      usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.TEXTURE_BINDING,
    });
    this.depthTexture = device.createTexture({
      size: [w, h],
      format: 'depth32float',
      usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_SRC | GPUTextureUsage.TEXTURE_BINDING,
    });
    // 只读副本：深度预渲染后拷贝，供 SSAO 采样，避免与主通道的附件写冲突。
    this.depthReadTexture = device.createTexture({
      size: [w, h],
      format: 'depth32float',
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
    });
    this.aoTexture = device.createTexture({
      size: [w, h], format: 'r8unorm',
      usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.TEXTURE_BINDING,
    });
    this.aoBlurTexture = device.createTexture({
      size: [w, h], format: 'r8unorm',
      usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.TEXTURE_BINDING,
    });
    this.aoSampler = device.createSampler({ magFilter: 'linear', minFilter: 'linear' });

    this.sceneBind = device.createBindGroup({
      layout: this.shadowPipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: { buffer: this.cameraUniform } },
        { binding: 1, resource: { buffer: this.dynamicUniform } },
        { binding: 2, resource: { buffer: this.aoUniform } },
      ],
    });
    // 主通道需要 5 个材质槽各一个绑定组（材质按 primitive 切换）。
    this.texBinds = this.textureSet.views.map((view, i) =>
      device.createBindGroup({
        layout: this.texLayout,
        entries: [
          { binding: 0, resource: view },
          { binding: 1, resource: this.textureSet.samplers[i] },
        ],
      }),
    );
    this.postBind = device.createBindGroup({
      layout: this.postLayout,
      entries: [
        { binding: 0, resource: this.shadowTexture.createView() },
        { binding: 1, resource: this.aoBlurTexture.createView() },
        { binding: 2, resource: this.aoSampler },
        { binding: 3, resource: { buffer: this.screenUniform } },
        { binding: 4, resource: { buffer: this.screenUniform } },
        { binding: 5, resource: this.depthReadTexture.createView() },
      ],
    });
    this.matBind = device.createBindGroup({
      layout: this.matLayout,
      entries: [{ binding: 0, resource: { buffer: this.materialUniform } }],
    });
    this.aoBind = device.createBindGroup({
      layout: this.sceneLayout,
      entries: [
        { binding: 0, resource: { buffer: this.cameraUniform } },
        { binding: 1, resource: { buffer: this.dynamicUniform } },
        { binding: 2, resource: { buffer: this.aoUniform } },
      ],
    });
    // SSAO 通道的 group2 复用 postBind（含 depthTex）。
    this.blurBind = device.createBindGroup({
      layout: this.aoTexLayout,
      entries: [
        { binding: 0, resource: this.aoTexture.createView() },
        { binding: 1, resource: this.aoSampler },
        { binding: 2, resource: { buffer: this.screenUniform } },
      ],
    });
    device.queue.writeBuffer(this.screenUniform, 0, new Float32Array([w, h, 0, 0]));
  }

  frameCamera(bounds) {
    // 按包围盒自动取景。
    const cx = (bounds.min[0] + bounds.max[0]) / 2;
    const cy = (bounds.min[1] + bounds.max[1]) / 2;
    const cz = (bounds.min[2] + bounds.max[2]) / 2;
    this.camera.target = [cx, cy, cz];
    const sx = bounds.max[0] - bounds.min[0];
    const sy = bounds.max[1] - bounds.min[1];
    const sz = bounds.max[2] - bounds.min[2];
    this.camera.dist = Math.max(sx, sy) * 1.6 + sz;
  }

  bindControls() {
    const c = this.canvas;
    let dragging = false;
    let last = [0, 0];
    c.addEventListener('pointerdown', (e) => {
      dragging = true; last = [e.clientX, e.clientY]; c.setPointerCapture(e.pointerId);
    });
    c.addEventListener('pointermove', (e) => {
      if (!dragging) return;
      const dx = e.clientX - last[0];
      const dy = e.clientY - last[1];
      last = [e.clientX, e.clientY];
      this.camera.yaw -= dx * 0.008;
      this.camera.pitch = Math.min(1.45, Math.max(0.12, this.camera.pitch + dy * 0.008));
    });
    c.addEventListener('pointerup', () => { dragging = false; });
    c.addEventListener('wheel', (e) => {
      e.preventDefault();
      this.camera.dist *= 1 + Math.sign(e.deltaY) * 0.08;
      this.camera.dist = Math.max(0.3, this.camera.dist);
    }, { passive: false });
  }

  setExplode(scale) { this.dynamic.explodeScale = scale; }
  setDisassemble(p) { this.dynamic.disassemble = Math.min(1, Math.max(0, p)); }

  render() {
    if (!this.device || !this.vertexBuffer) return;
    const device = this.device;
    const [w, h] = [this.canvas.width, this.canvas.height];

    const view = this.computeViewMatrix();
    const proj = perspective(Math.PI / 4.5, w / h, 0.05, 20);
    const viewProj = multiply(proj, view);
    const lightViewProj = this.computeLightMatrix();
    const camPos = this.eyePosition();

    // Camera 布局：viewProj(16) + lightViewProj(16) + camPos.xyz + pad(1)
    //              + lightDir.xyz + pad(1) = 40 floats。
    const camData = new Float32Array(40);
    camData.set(flatten(viewProj), 0);
    camData.set(flatten(lightViewProj), 16);
    camData.set(camPos, 32);
    camData.set([0.45, -0.6, -0.75], 36); // 平行光方向（指向光源的反方向约定）
    device.queue.writeBuffer(this.cameraUniform, 0, camData);

    const dynData = new Float32Array([
      this.dynamic.explodeScale, this.dynamic.disassemble,
      this.gpuData?.maxLayer ?? 1, this.frame * 0.016,
    ]);
    device.queue.writeBuffer(this.dynamicUniform, 0, dynData);

    const invProj = invert4(proj);
    const aoData = new Float32Array(36);
    aoData.set(flatten(proj), 0);
    aoData.set(flatten(invProj), 16);
    aoData.set([w, h], 32);
    aoData[34] = 0.12; // SSAO 采样半径（米）
    device.queue.writeBuffer(this.aoUniform, 0, aoData);

    const enc = device.createCommandEncoder();

    // 1) 阴影贴图。
    {
      const pass = enc.beginRenderPass({
        colorAttachments: [],
        depthStencilAttachment: {
          view: this.shadowTexture.createView(),
          depthClearValue: 1, depthLoadOp: 'clear', depthStoreOp: 'store',
        },
      });
      pass.setPipeline(this.shadowPipeline);
      pass.setBindGroup(0, this.sceneBind);
      pass.setVertexBuffer(0, this.vertexBuffer);
      pass.setIndexBuffer(this.indexBuffer, 'uint32');
      pass.drawIndexed(this.indexCount);
      pass.end();
    }
    // 2) 相机深度。
    {
      const pass = enc.beginRenderPass({
        colorAttachments: [],
        depthStencilAttachment: {
          view: this.depthTexture.createView(),
          depthClearValue: 1, depthLoadOp: 'clear', depthStoreOp: 'store',
        },
      });
      pass.setPipeline(this.depthPipeline);
      pass.setBindGroup(0, this.sceneBind);
      pass.setVertexBuffer(0, this.vertexBuffer);
      pass.setIndexBuffer(this.indexBuffer, 'uint32');
      pass.drawIndexed(this.indexCount);
      pass.end();
    }
    // 深度结果拷贝到只读副本，供后续 SSAO 采样。
    enc.copyTextureToTexture(
      { texture: this.depthTexture },
      { texture: this.depthReadTexture },
      [this.canvas.width, this.canvas.height, 1],
    );
    // 先提交深度/阴影通道，使深度纹理在后续 encoder 中可作为只读采样资源。
    device.queue.submit([enc.finish()]);
    const enc2 = device.createCommandEncoder();

    // 3) SSAO。
    {
      const pass = enc2.beginRenderPass({
        colorAttachments: [{ view: this.aoTexture.createView(), clearValue: [1, 1, 1, 1], loadOp: 'clear', storeOp: 'store' }],
      });
      pass.setPipeline(this.ssaoPipeline);
      pass.setBindGroup(0, this.aoBind);
      pass.setBindGroup(1, this.texBinds[0]);
      pass.setBindGroup(2, this.postBind);
      pass.setBindGroup(3, this.matBind);
      pass.draw(6);
      pass.end();
    }
    // 4) SSAO 模糊。
    {
      const pass = enc2.beginRenderPass({
        colorAttachments: [{ view: this.aoBlurTexture.createView(), clearValue: [1, 1, 1, 1], loadOp: 'clear', storeOp: 'store' }],
      });
      pass.setPipeline(this.blurPipeline);
      pass.setBindGroup(0, this.blurBind);
      pass.draw(6);
      pass.end();
    }
    // 5) 主 PBR 通道。
    {
      const pass = enc2.beginRenderPass({
        colorAttachments: [{ view: this.context.getCurrentTexture().createView(), clearValue: [0.93, 0.94, 0.95, 1], loadOp: 'clear', storeOp: 'store' }],
        depthStencilAttachment: {
          view: this.depthTexture.createView(),
          depthLoadOp: 'clear', depthStoreOp: 'store', depthClearValue: 1,
        },
      });
      pass.setPipeline(this.mainPipeline);
      pass.setBindGroup(0, this.sceneBind);
      pass.setBindGroup(2, this.postBind);
      pass.setBindGroup(3, this.matBind);
      pass.setVertexBuffer(0, this.vertexBuffer);
      pass.setIndexBuffer(this.indexBuffer, 'uint32');
      // 逐构件绘制（构件数 < 50，draw call 成本可忽略），每个构件绑定其首顶点材质。
      // 同一构件通常只有 1–2 个材质槽；混合材质构件按材质拆成子区间。
      for (const range of this.materialRanges) {
        pass.setBindGroup(1, this.texBinds[range.matId]);
        pass.drawIndexed(range.count, 1, range.start, 0, 0);
      }
      pass.end();
    }
    device.queue.submit([enc2.finish()]);
    this.frame++;
  }

  // 把每个构件的索引区间按材质拆成连续子区间（同一材质在构件内通常连续）。
  buildMaterialRanges(gpuData) {
    const ranges = [];
    for (const c of gpuData.components) {
      let run = null;
      for (let k = c.indexStart; k < c.indexStart + c.indexCount; k++) {
        const vertexIndex = gpuData.indices[k];
        const matId = gpuData.materials[vertexIndex];
        if (!run || run.matId !== matId) {
          if (run) ranges.push(run);
          run = { matId, start: k, count: 1 };
        } else {
          run.count++;
        }
      }
      if (run) ranges.push(run);
    }
    return ranges;
  }

  eyePosition() {
    const { yaw, pitch, dist, target } = this.camera;
    const cp = Math.cos(pitch);
    return [
      target[0] + dist * cp * Math.sin(yaw),
      target[1] + dist * cp * Math.cos(yaw),
      target[2] + dist * Math.sin(pitch),
    ];
  }

  computeViewMatrix() {
    const eye = this.eyePosition();
    return lookAt(eye, this.camera.target, [0, 0, 1]);
  }

  computeLightMatrix() {
    const b = this.gpuData?.bounds;
    if (!b) return identity4();
    const cx = (b.min[0] + b.max[0]) / 2;
    const cy = (b.min[1] + b.max[1]) / 2;
    const cz = (b.min[2] + b.max[2]) / 2;
    const radius = Math.max(b.max[0] - b.min[0], b.max[1] - b.min[1], b.max[2] - b.min[2]) * 0.8;
    const dir = normalize3([0.45, -0.6, -0.75]);
    const eye = [cx - dir[0] * radius * 2, cy - dir[1] * radius * 2, cz - dir[2] * radius * 2 + 0.4];
    const view = lookAt(eye, [cx, cy, cz], [0, 0, 1]);
    const s = radius * 1.4;
    const proj = ortho(-s, s, -s, s, 0.1, radius * 4);
    return multiply(proj, view);
  }
}

// ---------- 最小矩阵库（列主序，与 WGSL 对齐） ----------
function perspective(fovy, aspect, near, far) {
  const f = 1 / Math.tan(fovy / 2);
  const m = new Float32Array(16);
  m[0] = f / aspect; m[5] = f;
  m[10] = far / (near - far); m[11] = -1;
  m[14] = (far * near) / (near - far);
  return m;
}
function ortho(l, r, b, t, n, f) {
  const m = new Float32Array(16);
  m[0] = 2 / (r - l); m[5] = 2 / (t - b); m[10] = 1 / (n - f);
  m[12] = -(r + l) / (r - l); m[13] = -(t + b) / (t - b); m[14] = n / (n - f);
  m[15] = 1;
  return m;
}
function lookAt(eye, center, up) {
  const z = normalize3(sub3(eye, center));
  const x = normalize3(cross3(up, z));
  const y = cross3(z, x);
  const m = new Float32Array(16);
  m[0] = x[0]; m[1] = y[0]; m[2] = z[0]; m[3] = 0;
  m[4] = x[1]; m[5] = y[1]; m[6] = z[1]; m[7] = 0;
  m[8] = x[2]; m[9] = y[2]; m[10] = z[2]; m[11] = 0;
  m[12] = -dot3(x, eye); m[13] = -dot3(y, eye); m[14] = -dot3(z, eye); m[15] = 1;
  return m;
}
function multiply(a, b) {
  const out = new Float32Array(16);
  for (let c = 0; c < 4; c++) {
    for (let r = 0; r < 4; r++) {
      out[c * 4 + r] =
        a[r] * b[c * 4] + a[4 + r] * b[c * 4 + 1] +
        a[8 + r] * b[c * 4 + 2] + a[12 + r] * b[c * 4 + 3];
    }
  }
  return out;
}
function invert4(m) {
  const inv = new Float32Array(16);
  const a = [...m];
  inv[0] = a[5]*a[10]*a[15]-a[5]*a[11]*a[14]-a[9]*a[6]*a[15]+a[9]*a[7]*a[14]+a[13]*a[6]*a[11]-a[13]*a[7]*a[10];
  inv[4] = -a[4]*a[10]*a[15]+a[4]*a[11]*a[14]+a[8]*a[6]*a[15]-a[8]*a[7]*a[14]-a[12]*a[6]*a[11]+a[12]*a[7]*a[10];
  inv[8] = a[4]*a[9]*a[15]-a[4]*a[11]*a[13]-a[8]*a[5]*a[15]+a[8]*a[7]*a[13]+a[12]*a[5]*a[11]-a[12]*a[7]*a[9];
  inv[12] = -a[4]*a[9]*a[14]+a[4]*a[10]*a[13]+a[8]*a[5]*a[14]-a[8]*a[6]*a[13]-a[12]*a[5]*a[10]+a[12]*a[6]*a[9];
  inv[1] = -a[1]*a[10]*a[15]+a[1]*a[11]*a[14]+a[9]*a[2]*a[15]-a[9]*a[3]*a[14]-a[13]*a[2]*a[11]+a[13]*a[3]*a[10];
  inv[5] = a[0]*a[10]*a[15]-a[0]*a[11]*a[14]-a[8]*a[2]*a[15]+a[8]*a[3]*a[14]+a[12]*a[2]*a[11]-a[12]*a[3]*a[10];
  inv[9] = -a[0]*a[9]*a[15]+a[0]*a[11]*a[13]+a[8]*a[1]*a[15]-a[8]*a[3]*a[13]-a[12]*a[1]*a[11]+a[12]*a[3]*a[9];
  inv[13] = a[0]*a[9]*a[14]-a[0]*a[10]*a[13]-a[8]*a[1]*a[14]+a[8]*a[2]*a[13]+a[12]*a[1]*a[10]-a[12]*a[2]*a[9];
  inv[2] = a[1]*a[6]*a[15]-a[1]*a[7]*a[14]-a[5]*a[2]*a[15]+a[5]*a[3]*a[14]+a[13]*a[2]*a[7]-a[13]*a[3]*a[6];
  inv[6] = -a[0]*a[6]*a[15]+a[0]*a[7]*a[14]+a[4]*a[2]*a[15]-a[4]*a[3]*a[14]-a[12]*a[2]*a[7]+a[12]*a[3]*a[6];
  inv[10] = a[0]*a[5]*a[15]-a[0]*a[7]*a[13]-a[4]*a[1]*a[15]+a[4]*a[3]*a[13]+a[12]*a[1]*a[7]-a[12]*a[3]*a[5];
  inv[14] = -a[0]*a[5]*a[14]+a[0]*a[6]*a[13]+a[4]*a[1]*a[14]-a[4]*a[2]*a[13]-a[12]*a[1]*a[6]+a[12]*a[2]*a[5];
  inv[3] = -a[1]*a[6]*a[11]+a[1]*a[7]*a[10]+a[5]*a[2]*a[11]-a[5]*a[3]*a[10]-a[9]*a[2]*a[7]+a[9]*a[3]*a[6];
  inv[7] = a[0]*a[6]*a[11]-a[0]*a[7]*a[10]-a[4]*a[2]*a[11]+a[4]*a[3]*a[10]+a[8]*a[2]*a[7]-a[8]*a[3]*a[6];
  inv[11] = -a[0]*a[5]*a[11]+a[0]*a[7]*a[9]+a[4]*a[1]*a[11]-a[4]*a[3]*a[9]-a[8]*a[1]*a[7]+a[8]*a[3]*a[5];
  inv[15] = a[0]*a[5]*a[10]-a[0]*a[6]*a[9]-a[4]*a[1]*a[10]+a[4]*a[2]*a[9]+a[8]*a[1]*a[6]-a[8]*a[2]*a[5];
  let det = a[0]*inv[0]+a[1]*inv[4]+a[2]*inv[8]+a[3]*inv[12];
  for (let i = 0; i < 16; i++) inv[i] /= det;
  return inv;
}
function identity4() { const m = new Float32Array(16); m[0] = m[5] = m[10] = m[15] = 1; return m; }
function flatten(m) { return m; }
function sub3(a, b) { return [a[0]-b[0], a[1]-b[1], a[2]-b[2]]; }
function dot3(a, b) { return a[0]*b[0]+a[1]*b[1]+a[2]*b[2]; }
function cross3(a, b) {
  return [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]];
}
function normalize3(a) {
  const l = Math.hypot(a[0], a[1], a[2]) || 1;
  return [a[0]/l, a[1]/l, a[2]/l];
}

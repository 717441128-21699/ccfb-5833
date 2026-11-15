// 程序化材质：木纹与宋式「丹粉刷饰」彩画，运行时由 Canvas 生成，
// 不依赖任何外部图片资源。返回 ImageBitmap 供 copyExternalImageToTexture。

function makeCanvas(size) {
  const c = document.createElement('canvas');
  c.width = c.height = size;
  return c;
}

// 可重复的伪随机，保证每次重建纹理稳定。
function rng(seed) {
  let s = seed >>> 0;
  return () => {
    s = (s * 1664525 + 1013904223) >>> 0;
    return s / 4294967296;
  };
}

/// 木纹：暖棕底 + 纵向纤维 + 深浅年轮条纹 + 少量节疤。
export function makeWoodTexture(size = 512, base = [158, 115, 72]) {
  const c = makeCanvas(size);
  const ctx = c.getContext('2d');
  const rand = rng(20241115);
  const img = ctx.createImageData(size, size);

  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const wave =
        Math.sin(x * 0.05 + Math.sin(y * 0.02) * 2.2) * 14 +
        Math.sin(x * 0.19 + y * 0.004) * 7 +
        Math.sin(x * 0.61) * 3;
      const noise = (rand() - 0.5) * 12;
      const shade = wave + noise;
      const i = (y * size + x) * 4;
      img.data[i] = clamp255(base[0] + shade);
      img.data[i + 1] = clamp255(base[1] + shade * 0.82);
      img.data[i + 2] = clamp255(base[2] + shade * 0.6);
      img.data[i + 3] = 255;
    }
  }
  ctx.putImageData(img, 0, 0);

  // 节疤：几圈偏心椭圆。
  for (let k = 0; k < 5; k++) {
    const kx = rand() * size;
    const ky = rand() * size;
    const r = 12 + rand() * 22;
    ctx.save();
    ctx.translate(kx, ky);
    ctx.rotate(rand() * Math.PI);
    for (let ring = 0; ring < 4; ring++) {
      ctx.strokeStyle = `rgba(74,48,26,${0.18 - ring * 0.03})`;
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.ellipse(0, 0, r + ring * 5, (r + ring * 5) * 0.45, 0, 0, Math.PI * 2);
      ctx.stroke();
    }
    ctx.restore();
  }
  return c;
}

/// 彩画（丹粉刷饰简化版）：朱红底，叠晕条带 + 暖色团花 + 边线描金。
export function makeCaihuaTexture(size = 512) {
  const c = makeCanvas(size);
  const ctx = c.getContext('2d');
  const rand = rng(960115);

  const grad = ctx.createLinearGradient(0, 0, size, size);
  grad.addColorStop(0, '#8f1d16');
  grad.addColorStop(0.5, '#a8281d');
  grad.addColorStop(1, '#7c1712');
  ctx.fillStyle = grad;
  ctx.fillRect(0, 0, size, size);

  // 上下叠晕边带（由深到浅再到深）。
  const bands = [
    [0.06, '#5e120d'], [0.1, '#c8841f'], [0.14, '#e3b04a'],
    [0.86, '#e3b04a'], [0.9, '#c8841f'], [0.94, '#5e120d'],
  ];
  for (const [pos, color] of bands) {
    ctx.fillStyle = color;
    ctx.fillRect(0, pos * size, size, size * 0.022);
  }

  // 团花：六瓣花 + 花心，沿宽度方向重复排布。
  for (let fx = size * 0.18; fx < size; fx += size * 0.32) {
    for (let fy = size * 0.3; fy < size * 0.8; fy += size * 0.34) {
      drawFlower(ctx, fx + (rand() - 0.5) * 10, fy + (rand() - 0.5) * 10, size * 0.055);
    }
  }

  // 细碎锦地点缀。
  ctx.fillStyle = 'rgba(230,180,80,0.35)';
  for (let i = 0; i < 220; i++) {
    ctx.fillRect(rand() * size, rand() * size, 2, 2);
  }
  return c;
}

function drawFlower(ctx, x, y, r) {
  ctx.save();
  ctx.translate(x, y);
  for (let p = 0; p < 6; p++) {
    ctx.rotate(Math.PI / 3);
    ctx.fillStyle = 'rgba(233,178,74,0.92)';
    ctx.beginPath();
    ctx.ellipse(0, -r * 0.9, r * 0.42, r * 0.78, 0, 0, Math.PI * 2);
    ctx.fill();
    ctx.strokeStyle = 'rgba(94,18,13,0.55)';
    ctx.lineWidth = 1.2;
    ctx.stroke();
  }
  ctx.fillStyle = '#6d3a10';
  ctx.beginPath();
  ctx.arc(0, 0, r * 0.34, 0, Math.PI * 2);
  ctx.fill();
  ctx.restore();
}

function clamp255(v) {
  return Math.max(0, Math.min(255, v | 0));
}

/// 按材质槽生成对应的颜色贴图（木纹槽用木纹，彩画槽用彩画）。
export async function buildColorTextures(device, materialsDef, style) {
  const samplers = [];
  const views = [];
  for (const mat of materialsDef) {
    const canvas =
      style === 'caihua' && mat.painted
        ? makeCaihuaTexture()
        : makeWoodTexture(512, rgbFromLinear(mat.baseColor));
    const bitmap = await createImageBitmap(canvas);
    const tex = device.createTexture({
      size: [bitmap.width, bitmap.height],
      format: 'rgba8unorm-srgb',
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST | GPUTextureUsage.RENDER_ATTACHMENT,
    });
    device.queue.copyExternalImageToTexture({ source: bitmap }, { texture: tex }, [bitmap.width, bitmap.height]);
    const sampler = device.createSampler({
      magFilter: 'linear',
      minFilter: 'linear',
      addressModeU: 'repeat',
      addressModeV: 'repeat',
    });
    views.push(tex.createView());
    samplers.push(sampler);
    bitmap.close();
  }
  return { views, samplers };
}

// Rust 给的是线性基色，转回 sRGB 0–255 供 Canvas 调色。
function rgbFromLinear([r, g, b]) {
  return [r, g, b].map((v) => {
    const s = v <= 0.0031308 ? v * 12.92 : 1.055 * Math.pow(v, 1 / 2.4) - 0.055;
    return clamp255(s * 255);
  });
}

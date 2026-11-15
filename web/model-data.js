// 把 Rust 输出的 Assembly 烘焙成 WebGPU 渲染所需的扁平缓冲。
//
// 关键设计：顶点在 CPU 侧一次性烘焙到世界坐标（模型很小，千级三角面），
// 但每个顶点额外带 (componentId, layer, explodeDirXyz)，
// 爆炸/拆装位移全部在顶点着色器按 uniform 进度完成，
// 参数改动之外的动画不重建缓冲、不重新上传 GPU。

export function buildGpuBuffers(assembly) {
  const positions = [];
  const normals = [];
  const materials = [];
  const compIds = [];
  const layers = [];
  const explodeDirs = [];
  const indices = [];
  const componentMeta = [];

  let vertexOffset = 0;

  assembly.components.forEach((c, compId) => {
    const t = c.transform;
    const s = Math.sin(t.rotationZ);
    const q = Math.cos(t.rotationZ);
    const [tx, ty, tz] = t.translation;
    const m = c.mesh;
    const vcount = m.positions.length / 3;

    for (let v = 0; v < vcount; v++) {
      const lx = m.positions[v * 3];
      const ly = m.positions[v * 3 + 1];
      const lz = m.positions[v * 3 + 2];
      positions.push(q * lx - s * ly + tx, s * lx + q * ly + ty, lz + tz);

      const nx = m.normals[v * 3];
      const ny = m.normals[v * 3 + 1];
      const nz = m.normals[v * 3 + 2];
      normals.push(q * nx - s * ny, s * nx + q * ny, nz);

      materials.push(m.materialIds[v]);
      compIds.push(compId);
      layers.push(c.layer);
      explodeDirs.push(c.explodeDir[0], c.explodeDir[1], c.explodeDir[2]);
    }

    const localStart = indices.length;
    for (let i = 0; i < m.indices.length; i++) {
      indices.push(m.indices[i] + vertexOffset);
    }
    componentMeta.push({
      id: compId,
      name: c.name,
      kind: c.kind,
      layer: c.layer,
      indexStart: localStart,
      indexCount: m.indices.length,
    });
    vertexOffset += vcount;
  });

  const maxLayer = componentMeta.reduce((mx, c) => Math.max(mx, c.layer), 0);

  return {
    positions: new Float32Array(positions),
    normals: new Float32Array(normals),
    materials: new Uint16Array(materials),
    compIds: new Uint16Array(compIds),
    layers: new Uint16Array(layers),
    explodeDirs: new Float32Array(explodeDirs),
    indices: new Uint32Array(indices),
    components: componentMeta,
    maxLayer,
    bounds: assembly.bounds,
    materialsDef: assembly.materials,
    notes: assembly.notes,
  };
}

// 拆装因子：第 layer 层在整体进度 p∈[0,1] 下的位移比例。
// 每层在自己的时间窗内平滑抬升，层与层之间轻微重叠以保持连贯。
export function layerFactor(layer, maxLayer, progress) {
  if (maxLayer <= 0) return progress;
  const window = 1 / (maxLayer + 1);
  const start = layer * window * 0.85;
  const local = clamp((progress - start) / (window * 1.6), 0, 1);
  return smoothstep(local);
}

function clamp(x, a, b) {
  return Math.min(b, Math.max(a, x));
}
function smoothstep(x) {
  return x * x * (3 - 2 * x);
}

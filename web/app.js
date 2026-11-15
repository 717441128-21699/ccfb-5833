// 前端主入口：参数面板 → WASM 建模 → WebGPU 渲染，含 250ms 防抖重建与计时。

import { loadWasm, generateAssembly, generateGlb } from './wasm-loader.js';
import { buildGpuBuffers } from './model-data.js';
import { DougongRenderer } from './renderer.js';

const $ = (id) => document.getElementById(id);
const state = {
  params: {
    bracketSet: 'wu',
    caiGrade: 5,
    angStyle: 'xia',
    materialStyle: 'wood',
    includeInnerJumps: true,
  },
  rebuildTimer: null,
  anim: null,
};

async function main() {
  try {
    await loadWasm('./dougong_wasm.wasm');
  } catch (e) {
    showFatal(`WASM 模块加载失败：${e.message}`);
    return;
  }

  const canvas = $('canvas');
  resizeCanvas(canvas);
  window.addEventListener('resize', () => resizeCanvas(canvas));

  let renderer;
  try {
    renderer = new DougongRenderer(canvas);
    await renderer.init();
  } catch (e) {
    showFatal(e.message);
    return;
  }
  window.__renderer = renderer;

  bindUi(renderer);
  await rebuild(renderer, true);
}

function bindUi(renderer) {
  const bindSelect = (id, key) =>
    $(id).addEventListener('change', (e) => {
      state.params[key] =
        key === 'caiGrade' ? Number(e.target.value) : e.target.value;
      scheduleRebuild(renderer);
    });
  bindSelect('bracketSet', 'bracketSet');
  bindSelect('caiGrade', 'caiGrade');
  bindSelect('angStyle', 'angStyle');
  bindSelect('materialStyle', 'materialStyle');
  $('innerJumps').addEventListener('change', (e) => {
    state.params.includeInnerJumps = e.target.checked;
    scheduleRebuild(renderer);
  });

  $('explode').addEventListener('input', (e) => {
    const v = Number(e.target.value) / 100;
    $('explodeVal').textContent = `${e.target.value}%`;
    renderer.setExplode(v * 0.5);
  });
  $('disassemble').addEventListener('input', (e) => {
    const v = Number(e.target.value) / 100;
    $('disassembleVal').textContent = `${e.target.value}%`;
    renderer.setDisassemble(v);
  });

  $('assembleBtn').addEventListener('click', () => playDisassemble(renderer));
  $('resetCam').addEventListener('click', () => {
    if (renderer.gpuData) renderer.frameCamera(renderer.gpuData.bounds);
  });
  $('exportGlb').addEventListener('click', exportGlb);
}

// 250ms 防抖：连续拖动/切换时只在停顿后重建一次，满足「1 秒内重建」的交互预期。
function scheduleRebuild(renderer) {
  clearTimeout(state.rebuildTimer);
  state.rebuildTimer = setTimeout(() => rebuild(renderer, false), 250);
}

async function rebuild(renderer, first) {
  const t0 = performance.now();
  let assembly;
  try {
    assembly = generateAssembly(state.params);
  } catch (e) {
    showFatal(`建模失败：${e.message}`);
    return;
  }
  const tWasm = performance.now() - t0;

  const gpuData = buildGpuBuffers(assembly);
  await renderer.setModel(gpuData, state.params.materialStyle);
  const total = performance.now() - t0;

  $('statComponents').textContent = `构件 ${assembly.components.length} 个 · ${gpuData.maxLayer + 1} 个拆装层`;
  $('statTriangles').textContent = `三角面 ${assembly.components.reduce((s, c) => s + c.mesh.indices.length / 3, 0)}`;
  $('statRebuild').textContent = `WASM 推算 ${tWasm.toFixed(1)} ms · 总重建 ${total.toFixed(1)} ms`;

  $('notes').innerHTML = assembly.notes.map((n) => `<li>${n}</li>`).join('');
  renderLegend(assembly);

  if (!first) {
    flashStat(`重建完成 ${total.toFixed(0)} ms`);
  }
}

function renderLegend(assembly) {
  const kinds = new Map();
  for (const c of assembly.components) {
    kinds.set(c.kind, (kinds.get(c.kind) || 0) + 1);
  }
  const labels = {
    luDou: '栌斗', huaGong: '华栱', hengGong: '横栱',
    ang: '昂', shuaTou: '耍头', xiaoDou: '小斗',
  };
  const setLabels = {
    doukou: '斗口跳', doukouTiao: '斗口跳',
    si: '四铺作', siPuZuo: '四铺作',
    wu: '五铺作', wuPuZuo: '五铺作',
    liu: '六铺作', liuPuZuo: '六铺作',
    qi: '七铺作', qiPuZuo: '七铺作',
    ba: '八铺作', baPuZuo: '八铺作',
  };
  $('legend').innerHTML =
    `<b>${setLabels[assembly.params.bracketSet] || assembly.params.bracketSet}</b> · 材等 ${assembly.params.caiGrade}<br>` +
    [...kinds.entries()].map(([k, n]) => `${labels[k] || k} ×${n}`).join('　');
}

function playDisassemble(renderer) {
  if (state.anim) cancelAnimationFrame(state.anim);
  const slider = $('disassemble');
  const label = $('disassembleVal');
  const start = performance.now();
  const duration = 6500;
  // 0 → 1（逐层拆开）→ 停顿 → 0（装回）。
  const tick = (now) => {
    const t = ((now - start) % duration) / duration;
    let p;
    if (t < 0.42) p = t / 0.42;
    else if (t < 0.58) p = 1;
    else p = 1 - (t - 0.58) / 0.42;
    renderer.setDisassemble(p);
    slider.value = Math.round(p * 100);
    label.textContent = `${slider.value}%`;
    state.anim = requestAnimationFrame(tick);
  };
  state.anim = requestAnimationFrame(tick);
}

async function exportGlb() {
  try {
    const glb = generateGlb(state.params);
    const blob = new Blob([glb], { type: 'model/gltf-binary' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `dougong_${state.params.bracketSet}_grade${state.params.caiGrade}.glb`;
    a.click();
    URL.revokeObjectURL(url);
  } catch (e) {
    showFatal(`导出失败：${e.message}`);
  }
}

function resizeCanvas(canvas) {
  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  const rect = canvas.parentElement.getBoundingClientRect();
  canvas.width = Math.max(320, Math.floor(rect.width * dpr));
  canvas.height = Math.max(240, Math.floor(rect.height * dpr));
  if (window.__renderer?.gpuData) window.__renderer.createFrameResources();
}

let flashTimer = null;
function flashStat(text) {
  const el = $('statRebuild');
  el.style.color = '#7fc97f';
  el.textContent = `${text}（目标 < 1000 ms）`;
  clearTimeout(flashTimer);
  flashTimer = setTimeout(() => { el.style.color = ''; }, 1200);
}

function showFatal(msg) {
  $('overlay').classList.remove('hidden');
  $('overlayMsg').textContent =
    msg + '。请使用支持 WebGPU 的 Chrome/Edge 113+ 并通过 http(s) 访问（不要 file:// 直开）。';
}

main();

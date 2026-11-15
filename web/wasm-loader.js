// WASM 模块加载与 FFI 协议封装。
// 协议见 crates/dougong-wasm/src/lib.rs：
//   dg_malloc/dg_free 管理输入区；dg_generate / dg_generate_glb 把结果放在
//   模块持有的静态槽中，dg_last_ptr / dg_last_len 取址，前 8 字节为长度头。

let instance = null;

export async function loadWasm(url = './dougong_wasm.wasm') {
  if (instance) return instance;
  const resp = await fetch(url);
  if (!resp.ok) throw new Error(`WASM 加载失败: ${resp.status}`);
  const bytes = await resp.arrayBuffer();
  const result = await WebAssembly.instantiate(bytes, {});
  instance = result.instance.exports;
  return instance;
}

function callRaw(fn, params) {
  const ex = instance;
  const mem = ex.memory;
  const input = new TextEncoder().encode(JSON.stringify(params));
  const ptr = ex.dg_malloc(input.length);
  new Uint8Array(mem.buffer).set(input, ptr);
  const totalLen = fn(ptr, input.length);
  const outPtr = ex.dg_last_ptr();
  const dv = new DataView(mem.buffer);
  const payloadLen = dv.getUint32(outPtr, true);
  // 拷贝出来再释放输入区，避免后续调用替换静态槽。
  const payload = new Uint8Array(mem.buffer.slice(outPtr + 8, outPtr + 8 + payloadLen));
  ex.dg_free(ptr, input.length);
  if (totalLen === 0) throw new Error('WASM 内部异常');
  return payload;
}

export function generateAssembly(params) {
  const payload = callRaw(instance.dg_generate, params);
  const text = new TextDecoder().decode(payload);
  const data = JSON.parse(text);
  if (data.error) throw new Error(data.error);
  return data;
}

export function generateGlb(params) {
  const payload = callRaw(instance.dg_generate_glb, params);
  // 出错时槽内是 JSON 而非 GLB（魔数 glTF 不匹配）。
  if (payload.length < 4 || new TextDecoder().decode(payload.slice(0, 4)) !== 'glTF') {
    const err = JSON.parse(new TextDecoder().decode(payload));
    throw new Error(err.error || 'GLB 导出失败');
  }
  return payload;
}

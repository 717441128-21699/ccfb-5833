//! WASM 边界：纯 C ABI，不依赖 wasm-bindgen。
//!
//! 协议（前端见 web/wasm-loader.js）：
//! 1. [`dg_malloc`] 分配输入区，前端拷入 UTF-8 参数 JSON；
//! 2. 调 [`dg_generate`] / [`dg_generate_glb`]，返回值 0 = 成功；
//! 3. [`dg_last_ptr`] / [`dg_last_len`] 取结果缓冲区（前 8 字节为
//!    `[payload_len:u32, reserved:u32]`，其后是 UTF-8 JSON 或 GLB 字节）；
//! 4. 前端拷贝后调 [`dg_free`] 归还输入区；结果缓冲区由模块持有，
//!    下次调用时自动替换，前端无需释放。

use dougong_core::{generate_glb, generate_json};
use std::mem;
use std::sync::Mutex;

/// 最近一次调用的结果缓冲（wasm32 单线程，Mutex 仅为满足安全可变静态）。
static LAST: Mutex<Option<Vec<u8>>> = Mutex::new(None);

/// 在 WASM 线性内存中分配 `len` 字节，返回指针（0 表示失败）。
#[no_mangle]
pub extern "C" fn dg_malloc(len: u32) -> u32 {
    let mut buf: Vec<u8> = Vec::with_capacity(len as usize);
    let ptr = buf.as_mut_ptr() as u32;
    mem::forget(buf);
    ptr
}

/// 归还 [`dg_malloc`] 分配的内存。
///
/// # Safety
/// `ptr` 必须来自 [`dg_malloc`]，`len` 与分配长度一致，且仅释放一次。
#[no_mangle]
pub unsafe extern "C" fn dg_free(ptr: u32, len: u32) {
    if ptr == 0 || len == 0 {
        return;
    }
    drop(Vec::from_raw_parts(
        ptr as *mut u8,
        len as usize,
        len as usize,
    ));
}

fn store_result(mut payload: Vec<u8>) -> u32 {
    let mut packed = Vec::with_capacity(payload.len() + 8);
    packed.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    packed.extend_from_slice(&0u32.to_le_bytes());
    packed.append(&mut payload);
    let code = packed.len() as u32;
    *LAST.lock().unwrap() = Some(packed);
    code
}

fn error_result(msg: String) -> u32 {
    store_result(serde_json::json!({ "error": msg }).to_string().into_bytes())
}

/// 参数 JSON（ptr/len 指向 WASM 内存中的 UTF-8）→ 装配 JSON。
/// 返回结果缓冲区总长度（含 8 字节头），0 表示内部异常。
#[no_mangle]
pub extern "C" fn dg_generate(ptr: u32, len: u32) -> u32 {
    let input = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    let text = match std::str::from_utf8(input) {
        Ok(t) => t,
        Err(e) => return error_result(format!("输入不是 UTF-8: {e}")),
    };
    match generate_json(text) {
        Ok(json) => store_result(json.into_bytes()),
        Err(e) => error_result(e),
    }
}

/// 参数 JSON → GLB；出错时槽内为错误 JSON（前端按 glTF 魔数区分）。
#[no_mangle]
pub extern "C" fn dg_generate_glb(ptr: u32, len: u32) -> u32 {
    let input = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    let text = match std::str::from_utf8(input) {
        Ok(t) => t,
        Err(e) => return error_result(format!("输入不是 UTF-8: {e}")),
    };
    match generate_glb(text) {
        Ok(glb) => store_result(glb),
        Err(e) => error_result(e),
    }
}

/// 最近一次结果缓冲区在线性内存中的指针。
#[no_mangle]
pub extern "C" fn dg_last_ptr() -> u32 {
    let guard = LAST.lock().unwrap();
    guard.as_ref().map(|v| v.as_ptr() as u32).unwrap_or(0)
}

/// 最近一次结果缓冲区的总长度（含 8 字节头）。
#[no_mangle]
pub extern "C" fn dg_last_len() -> u32 {
    let guard = LAST.lock().unwrap();
    guard.as_ref().map(|v| v.len() as u32).unwrap_or(0)
}

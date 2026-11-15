# 参数化斗拱建模系统（Rust/WASM + WebGPU）

按《营造法式》材分制，输入铺作类型、材等、跳数、昂制等参数，自动生成栌斗、华栱、
昂、耍头等构件几何并按榫卯关系装配，支持爆炸视图、逐层拆装动画、木纹/彩画材质、
阴影与环境光遮蔽，可导出标准 glTF(.glb)。

## 目录结构

```
crates/dougong-core/   纯 Rust 建模核心：材分制、构件几何、装配、GLB 导出（可原生测试）
crates/dougong-wasm/   C ABI 的 WASM 包装（cdylib，无 wasm-bindgen）
web/                   WebGPU 前端（渲染器、程序化材质、参数面板）
scripts/               headless Chrome 自测脚本
docs/DESIGN.md         法式依据与判断值说明
```

## 构建

```bash
# 1. 编译 WASM（需要 rustup target add wasm32-unknown-unknown）
cargo build -p dougong-wasm --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/dougong_wasm.wasm web/

# 2. 起静态服务器（WASM 要求 http(s)，不能 file:// 直开）
cd web && python3 -m http.server 8741
# 浏览器打开 http://localhost:8741/index.html （需 Chrome/Edge 113+ 支持 WebGPU）
```

或直接 `./scripts/build.sh` 完成编译+拷贝。

## 测试

```bash
cargo test -p dougong-core     # Rust 单元测试（材分尺度/铺作/GLB/耗时预算）
node scripts/cdp-test.mjs      # headless Chrome 跑 WebGPU 端到端（需先起服务器）
```

## 参数

| 参数 | 取值 |
|---|---|
| 铺作类型 | 斗口跳 / 四铺作(一跳) / 五铺作(两跳) / 六铺作(三跳) / 七铺作(四跳) / 八铺作(五跳) |
| 材等 | 一至八等材 |
| 昂制 | 下昂 / 上昂 / 卷头造（不用昂） |
| 材质 | 原木木纹 / 宋式彩画（丹粉刷饰） |
| 里跳 | 生成室内一侧，或只做外跳半朵 |

法式未规定死数的推算（昂斜率、昂尾长、耍头削斜、榫卯表达等）见 `docs/DESIGN.md`。

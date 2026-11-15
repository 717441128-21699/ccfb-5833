#!/usr/bin/env bash
# 编译 WASM 并拷贝到 web 目录，然后跑核心单元测试。
set -euo pipefail
cd "$(dirname "$0")/.."

cargo test -p dougong-core
cargo build -p dougong-wasm --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/dougong_wasm.wasm web/dougong_wasm.wasm
echo "✔ WASM 已更新到 web/dougong_wasm.wasm"

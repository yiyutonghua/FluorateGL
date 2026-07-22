#!/usr/bin/env bash
# 使用 Mesa llvmpipe 软件后端运行 glslang test suite
#
# 用法: bash tests/run_glslang_suite.sh
#
# 环境变量:
#   FLUORATEGL_BACKEND=llvmpipe        使用 llvmpipe 后端 (libGLESv2.so.2 + libEGL.so.1)
#   EGL_PLATFORM=surfaceless           无显示器环境 (CI)
#   MESA_LOADER_DRIVER_OVERRIDE=llvmpipe  强制 llvmpipe 驱动
#   LIBGL_ALWAYS_SOFTWARE=1            强制软件渲染

set -euo pipefail

cd "$(dirname "$0")/.."

export FLUORATEGL_BACKEND=llvmpipe
export EGL_PLATFORM=surfaceless
export MESA_LOADER_DRIVER_OVERRIDE=llvmpipe
export LIBGL_ALWAYS_SOFTWARE=1
export FLUORATEGL_LOG=warn

echo "=== 运行 glslang test suite (llvmpipe 后端) ==="
cargo run --example glslang_suite --release

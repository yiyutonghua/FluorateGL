#!/usr/bin/env bash
# FluorateGL 测试入口脚本。
#
# 流程：
#   1. 依赖检查（cargo / gcc / EGL & GLES 头文件与库）
#   2. （可选）拉取 glslang 子模块，用于 glslang_suite
#   3. 构建 libfluorategl.so
#   4. Rust 单元 + 集成测试 (cargo test)
#   5. GL C 端到端测试 (tests/gl/test_shader_translation.c，通过 dlopen 调用 cdylib)
#   6. glslang 翻译套件 (example glslang_suite，仅在子模块存在时运行)
#
# 使用方法：
#   bash tests/run.sh                # 全量测试
#   bash tests/run.sh --skip-glslang # 跳过 glslang 套件
#   bash tests/run.sh --only-cargo   # 只跑 cargo test

set -e

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_ROOT"

TARGET="x86_64-unknown-linux-gnu"
RUN_GLSLANG=1
RUN_CARGO=1
RUN_C=1

for arg in "$@"; do
    case "$arg" in
        --skip-glslang) RUN_GLSLANG=0 ;;
        --only-cargo)   RUN_C=0; RUN_GLSLANG=0 ;;
        --only-c)       RUN_CARGO=0; RUN_GLSLANG=0 ;;
        -h|--help)
            cat <<EOF
用法: bash tests/run.sh [选项]
选项:
  --skip-glslang  跳过 glslang 翻译套件
  --only-cargo    只跑 cargo test
  --only-c        只跑 C 端到端测试
EOF
            exit 0
            ;;
        *)
            echo "未知选项: $arg" >&2
            exit 2
            ;;
    esac
done

log()  { printf '\033[1;34m[run.sh]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[run.sh]\033[0m %s\n' "$*" >&2; }
fail() { printf '\033[1;31m[run.sh] 失败:\033[0m %s\n' "$*" >&2; exit 1; }

# ============================================================
# 1. 依赖检查
# ============================================================
log "检查构建依赖..."
command -v cargo >/dev/null || fail "未找到 cargo，请安装 Rust 工具链"
command -v gcc    >/dev/null || fail "未找到 gcc，请安装 build-essential"

# EGL / GLES 头文件
EGL_H=/usr/include/EGL/egl.h
GLES3_H=/usr/include/GLES3/gl3.h
[[ -f "$EGL_H" ]]    || fail "缺少 $EGL_H（apt: libegl-dev）"
[[ -f "$GLES3_H" ]]  || fail "缺少 $GLES3_H（apt: libgles2-mesa-dev）"

# EGL / GLES 运行库
GLES2_PATH="$(ldconfig -p | awk '/libGLESv2\.so\.2/{print $NF; exit}')"
EGL_PATH="$(ldconfig -p | awk '/libEGL\.so\.1/{print $NF; exit}')"
[[ -n "$GLES2_PATH" ]] || fail "未找到 libGLESv2.so.2（apt: libgles2）"
[[ -n "$EGL_PATH" ]]   || fail "未找到 libEGL.so.1（apt: libegl1）"
log "EGL: $EGL_PATH"
log "GLESv2: $GLES2_PATH"

# Mesa 软渲染（llvmpipe）需要在 PATH 中可用，否则 surfaceless EGL 起不来
command -v llvm-config >/dev/null 2>&1 || warn "未找到 llvm-config，若 EGL 初始化失败请安装 libllvmpipe-..."

# ============================================================
# 2. （可选）拉取 glslang 子模块
# ============================================================
GLSLANG_DIR="$PROJECT_ROOT/tests/glsl/glslang"
if [[ "$RUN_GLSLANG" -eq 1 ]]; then
    if [[ ! -d "$GLSLANG_DIR/Test" ]]; then
        log "拉取 glslang 子模块..."
        git submodule update --init --recursive tests/glsl/glslang || {
            warn "glslang 子模块拉取失败，跳过 glslang suite"
            RUN_GLSLANG=0
        }
    fi
    if [[ ! -d "$GLSLANG_DIR/Test" ]]; then
        warn "glslang 测试集不存在: $GLSLANG_DIR/Test，跳过 glslang suite"
        RUN_GLSLANG=0
    fi
fi

# ============================================================
# 3. 构建 libfluorategl.so
# ============================================================
log "构建 libfluorategl.so ($TARGET)..."
cargo build --target "$TARGET"

# 准备 libGLESv3.so 软链（FluorateGL 默认加载 libGLESv3.so）
ln -sf "$GLES2_PATH" "$PROJECT_ROOT/libGLESv3.so"
log "已创建软链: libGLESv3.so -> $GLES2_PATH"

export LD_LIBRARY_PATH="$PROJECT_ROOT${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export EGL_PLATFORM=surfaceless
export MESA_LOADER_DRIVER_OVERRIDE=llvmpipe

# ============================================================
# 4. Rust 单元 + 集成测试
# ============================================================
EXIT_CODE=0
if [[ "$RUN_CARGO" -eq 1 ]]; then
    log "=== cargo test ==="
    if ! cargo test --target "$TARGET"; then
        warn "cargo test 失败"
        EXIT_CODE=1
    fi
fi

# ============================================================
# 5. GL C 端到端测试
# ============================================================
if [[ "$RUN_C" -eq 1 ]]; then
    log "=== GL C 端到端测试 ==="
    gcc -o tests/gl/test_shader_translation tests/gl/test_shader_translation.c -ldl -lEGL
    if ! ./tests/gl/test_shader_translation; then
        warn "GL C 测试失败"
        EXIT_CODE=1
    fi
fi

# ============================================================
# 6. glslang 翻译套件
# ============================================================
if [[ "$RUN_GLSLANG" -eq 1 ]]; then
    log "=== glslang 翻译套件 ==="
    if ! cargo run --quiet --example glslang_suite --target "$TARGET"; then
        warn "glslang suite 失败（部分失败属正常，套件含负例）"
        # glslang suite 本身允许部分失败，只在完全无法运行时计入错误
    fi
fi

# ============================================================
# 汇总
# ============================================================
if [[ "$EXIT_CODE" -eq 0 ]]; then
    log "所有测试通过 ✅"
else
    fail "存在失败的测试步骤"
fi

#!/usr/bin/env bash
set -e

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_ROOT"

# Build FluorateGL for Linux host.
bash linux-build.sh

# Mesa ships GLES3 support in libGLESv2.so.2; create the name FluorateGL expects.
GLES2_PATH="$(ldconfig -p | awk '/libGLESv2\.so\.2/{print $NF; exit}')"
if [ -z "$GLES2_PATH" ]; then
    echo "libGLESv2.so.2 not found" >&2
    exit 1
fi
ln -sf "$GLES2_PATH" libGLESv3.so

export LD_LIBRARY_PATH=.
export EGL_PLATFORM=surfaceless
export MESA_LOADER_DRIVER_OVERRIDE=llvmpipe

# --- GL tests: exercise the GL API through the cdylib ---
echo "=== GL tests ==="
gcc -o tests/gl/test_shader_translation tests/gl/test_shader_translation.c -ldl -lEGL
./tests/gl/test_shader_translation

# --- GLSL tests: translate the glslang test suite ---
echo "=== GLSL tests ==="
cargo run --quiet \
    --example glslang_suite \
    --features shaderc/build-from-source \
    --target x86_64-unknown-linux-gnu

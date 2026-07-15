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

# Build the native test harness.
gcc -o tests/test_shader_translation tests/test_shader_translation.c -ldl -lEGL

# Run with Mesa/llvmpipe surfaceless GLES.
EGL_PLATFORM=surfaceless \
LD_LIBRARY_PATH=. \
MESA_LOADER_DRIVER_OVERRIDE=llvmpipe \
./tests/test_shader_translation

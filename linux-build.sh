#!/usr/bin/env bash
set -e

# Build FluorateGL for Linux host (x86_64).
#
# The prebuilt shaderc static library under src/shaderc/ is for Android ARM64,
# so on Linux host we force shaderc-rs to build shaderc from source.

export SHADERC_LIB_DIR=""

cargo build \
    --target x86_64-unknown-linux-gnu \
    "$@"

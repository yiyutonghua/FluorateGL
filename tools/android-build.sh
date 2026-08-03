#!/usr/bin/env bash
set -e

cd  ..
# Build FluorateGL for android host (aarch64).

cargo build \
    --target aarch64-linux-android \
    --features build-from-source \
    "$@"

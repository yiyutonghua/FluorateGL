#!/usr/bin/env bash
set -e

# Build FluorateGL for Linux host (x86_64).

cargo build \
    --target x86_64-unknown-linux-gnu \
    "$@"

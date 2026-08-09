#!/usr/bin/env bash
# SPIRV-Tools 优化管线性能基准（S3-2）
#
# 对比 shaderc-only 与 shaderc+opt 的耗时 / SPIR-V word 数 / GLES 输出行数。
# 用法：bash tools/bench_opt.sh
set -e
cd "$(dirname "$0")/.."
echo "=== 构建 bench_opt（debug）==="
cargo build --quiet --example bench_opt
echo "=== 运行基准（5 样本 × 3 次平均）==="
cargo run --quiet --example bench_opt

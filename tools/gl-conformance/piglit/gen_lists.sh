#!/usr/bin/env bash
# gen_lists.sh —— 用 piglit print-cmd 在 CI host 枚举用例清单
# （抄自 MobileGL tools/piglit-android/README.md 的 print-cmd 枚举方式）
#
# 用法：
#   PIGLIT_ROOT=/path/to/piglit PIGLIT_BUILD_DIR=/path/to/piglit/build \
#     PYTHON=/path/to/venv/bin/python ./gen_lists.sh [--ci] [OUT]
#
# 产出（"name ::: command" 行，供 run_piglit_local.py --list 使用）：
#   gl33-full.list  全量 GL 3.x + GLSL 3.30（~15k 测试；本地/夜间跑）
#   gl33-ci.list    精选子集（--ci；CI 跑，控制时长）
#
# 组名用 @ 分隔（spec@!opengl 3.3@minmax）。版本组（spec@!opengl 1.x…3.3）、
# GLSL 组（spec@glsl-1.10…3.30）以及并入 GL 3.1–3.3 core 的 ARB 扩展组
# 合起来是完整的 "GL 3.3 core" 套件。filter 是 piglit -t 正则：
#   - full:  spec@!opengl 3[.]（3.0/3.1/3.2/3.3）+ spec@glsl-3[.]30
#   - ci:    聚焦最高版本组 spec@!opengl 3.3 + spec@glsl-3.30，
#            枚举后按需人工裁剪（基线覆盖优先，见 README）
set -euo pipefail

PIGLIT_ROOT=${PIGLIT_ROOT:-$PWD/piglit}
PIGLIT_BUILD_DIR=${PIGLIT_BUILD_DIR:-$PIGLIT_ROOT/build}
PYTHON=${PYTHON:-python3}

CI=0
if [[ "${1:-}" == "--ci" ]]; then CI=1; shift; fi
OUT="${1:-}"
if [[ -z "$OUT" ]]; then
  OUT="$PWD/gl33-$([ $CI = 1 ] && echo ci || echo full).list"
fi

FILTERS=(-t "spec@!opengl 3.3" -t "spec@glsl-3.30")
[ $CI = 1 ] || FILTERS=(-t "spec@!opengl 3[.]" -t "spec@glsl-3[.]30")

echo "piglit root:  $PIGLIT_ROOT"
echo "build dir:    $PIGLIT_BUILD_DIR"
echo "python:       $PYTHON"
echo "profile(s):   opengl shader glslparser"
echo "filter(s):    ${FILTERS[*]}"

cd "$PIGLIT_ROOT"
for prof in opengl shader glslparser; do
  PIGLIT_BUILD_DIR="$PIGLIT_BUILD_DIR" "$PYTHON" ./piglit print-cmd \
    "${FILTERS[@]}" "$prof"
done > "$OUT"

echo "wrote:        $OUT ($(wc -l < "$OUT") 行)"

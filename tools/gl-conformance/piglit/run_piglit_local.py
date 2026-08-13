#!/usr/bin/env python3
# FluorateGL - tools/gl-conformance/piglit/run_piglit_local.py
# 抄自 MobileGL tools/piglit-android/run_piglit_android.py（去 Android 化改写：
# 去掉 adb/推送/imagereader，改为本机串行执行 + timeout + 结果解析）
# Copyright (c) 2025-2026 FluorateGL-Dev
# Licensed under the GNU Lesser General Public License v3.0:
#   https://www.gnu.org/licenses/gpl-3.0.txt
#   https://www.gnu.org/licenses/lgpl-3.0.txt
# SPDX-License-Identifier: LGPL-3.0-only
# End of Source File Header
"""Run piglit GL tests locally against FluorateGL.

Pipeline:
  1. Read a test list produced by `piglit print-cmd` ("name ::: command" lines).
  2. Rewrite host paths for a local run (binaries/data stay under the piglit
     checkout).
  3. Execute tests serially in chunked shell scripts under `timeout`, with
     FluorateGL served to waffle via WAFFLE_EGL_LIBRARY/WAFFLE_GL_LIBRARY
     (waffle dlopens libfluorategl.so directly; no LD_PRELOAD).
  4. Parse "PIGLIT: {...}" result lines, classify, and write results + summary.

Optional sanity gate: run `wflinfo` against the same environment before the
suite to verify the whole stack (waffle -> libfluorategl.so -> llvmpipe)
actually produces a GL 3.3 core context.
"""

import argparse
import json
import os
import re
import shlex
import subprocess
import sys
import tempfile
import time
from pathlib import Path

RESULT_LINE = re.compile(rb'PIGLIT: *({.*})')
MARK_START = re.compile(rb'@@@T (\d+) START')
MARK_EXIT = re.compile(rb'@@@T (\d+) EXIT (\d+)')

# GNU coreutils timeout exit codes: 124 = timed out (SIGTERM),
# 137 = SIGKILL after -k, 142 = SIGALRM.
TIMEOUT_EXITS = {124, 137, 142}

# 环境组与 tools/gl-conformance/common/env.sh 保持一致：
# - FLUORATEGL_BACKEND=llvmpipe：目标库内部后端（CI 无 GPU，Mesa llvmpipe）
# - EGL_PLATFORM=surfaceless + MESA_LOADER_DRIVER_OVERRIDE=llvmpipe：无窗口平台
# - LIBGL_ALWAYS_SOFTWARE=1：软件渲染兜底
ENV_GROUP = {
    'FLUORATEGL_BACKEND': 'llvmpipe',
    'EGL_PLATFORM': 'surfaceless',
    'MESA_LOADER_DRIVER_OVERRIDE': 'llvmpipe',
    'LIBGL_ALWAYS_SOFTWARE': '1',
}


def parse_list(path):
    tests = []
    for line in Path(path).read_text().splitlines():
        line = line.strip()
        if not line or ' ::: ' not in line:
            continue
        name, cmd = line.split(' ::: ', 1)
        tests.append((name.strip(), shlex.split(cmd)))
    return tests


def rewrite_cmd(argv, piglit_root, build_dir):
    """Map host paths in a test command to local absolute paths.

    Serialized profiles record data paths under the build dir, but only
    *generated* data lives there in an out-of-tree build; plain test files
    stay in the source tests/ dir.
    """
    build_abs = str((piglit_root / build_dir).resolve())
    src_abs = str(piglit_root.resolve())
    out = []
    for i, arg in enumerate(argv):
        a = arg
        if i == 0:
            # program: "build/bin/foo" or absolute
            prog = a if os.path.isabs(a) else str((piglit_root / a).resolve())
            out.append(prog)
            continue
        p = a if os.path.isabs(a) else None
        if p and p.startswith(build_abs + '/generated_tests/'):
            out.append(p)
        elif p and p.startswith(build_abs + '/tests/'):
            if not os.path.exists(p):
                p = p.replace(build_abs + '/tests', src_abs + '/tests', 1)
            out.append(p)
        elif p and p.startswith(src_abs + '/tests/'):
            out.append(p)
        else:
            out.append(a)
    return out


def fluorategl_env(library, piglit_root, extra_lib_dirs):
    """Environment for the test processes.

    WAFFLE_EGL_LIBRARY/WAFFLE_GL_LIBRARY point waffle's dlopen at
    libfluorategl.so (absolute path, no LD_LIBRARY_PATH tricks needed);
    LD_LIBRARY_PATH carries the waffle install dir so the piglit test
    binaries can load libwaffle-1.so.
    """
    lib = str(library.resolve())
    lib_dirs = [str(d) for d in extra_lib_dirs] + [os.path.dirname(lib)]
    env = {
        'LD_LIBRARY_PATH': os.pathsep.join(lib_dirs),
        'WAFFLE_EGL_LIBRARY': lib,
        'WAFFLE_GL_LIBRARY': lib,
        # Upgrade low compat context requests to what FluorateGL implements;
        # without this, piglit's supports_gl_compat_version=10 tests get the
        # raw backend version string and skip themselves.
        'WAFFLE_FORCE_GL_CONTEXT_VERSION': '33core',
        'PIGLIT_PLATFORM': 'surfaceless_egl',
        'PIGLIT_SOURCE_DIR': str(piglit_root),
    }
    env.update(ENV_GROUP)
    return env


def wflinfo_check(wflinfo, env):
    """Sanity gate: verify waffle -> libfluorategl.so -> llvmpipe produces a
    GL 3.3 core context before burning time on the full suite."""
    cmd = [wflinfo, '--platform', 'surfaceless_egl', '--api', 'gl',
           '--version', '3.3', '--profile', 'core']
    print(f'  wflinfo: {" ".join(cmd)}')
    try:
        r = subprocess.run(cmd, env=env, capture_output=True, timeout=60)
    except subprocess.TimeoutExpired:
        sys.exit('wflinfo timed out — waffle/FluorateGL 加载卡死，检查 --library/--waffle-dir')
    out = (r.stdout + r.stderr).decode(errors='replace')
    print(out.strip()[:500])
    if r.returncode != 0:
        sys.exit(f'wflinfo 失败 (exit {r.returncode}) — 栈未就绪，检查库路径与环境')
    if 'OpenGL version string' not in out or '3.3' not in out:
        sys.exit('wflinfo 未报告 OpenGL 3.3 — 上下文创建异常，见上方输出')


def classify(exit_code, piglit_results, timed_out):
    if timed_out:
        return 'timeout'
    result = None
    for r in piglit_results:
        if 'result' in r:
            result = r['result']
    if result is None:
        return 'crash' if exit_code != 0 else 'notrun'
    if exit_code not in (0, 1):
        return 'crash'
    return result


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--piglit-root', required=True, type=Path,
                    help='piglit source checkout (with the local build dir inside)')
    ap.add_argument('--build-dir', default='build',
                    help='build dir name inside piglit root')
    ap.add_argument('--list', required=True,
                    help='test list file from `piglit print-cmd` (name ::: cmd)')
    ap.add_argument('--library', required=True, type=Path,
                    help='path to libfluorategl.so (dlopen target for waffle)')
    ap.add_argument('--waffle-dir', default=None, type=Path,
                    help='directory containing libwaffle-1.so (added to '
                         'LD_LIBRARY_PATH); omit if waffle is installed system-wide')
    ap.add_argument('--extra-lib-dir', action='append', default=[], type=Path,
                    help='extra dirs for LD_LIBRARY_PATH (repeatable)')
    ap.add_argument('--out', required=True, type=Path, help='results directory')
    ap.add_argument('--timeout', type=int, default=60,
                    help='per-test timeout (seconds)')
    ap.add_argument('--chunk', type=int, default=200,
                    help='tests per chunked shell script')
    ap.add_argument('--wflinfo', default=None, type=Path,
                    help='wflinfo binary (waffle build); run the sanity check before the suite')
    args = ap.parse_args()

    piglit_root = args.piglit_root.resolve()
    args.out.mkdir(parents=True, exist_ok=True)

    tests = parse_list(args.list)
    if not tests:
        print('no tests in list', file=sys.stderr)
        return 2

    print(f'[1/3] {len(tests)} tests; rewriting paths')
    cmds = []
    for name, argv in tests:
        cmds.append((name, rewrite_cmd(argv, piglit_root, args.build_dir)))

    env = fluorategl_env(args.library, piglit_root,
                         ([args.waffle_dir] if args.waffle_dir else []) + args.extra_lib_dir)
    env_lines = '\n'.join(f'export {k}={shlex.quote(v)}' for k, v in env.items())

    if args.wflinfo:
        print('[2/3] wflinfo sanity check')
        wflinfo_check(str(args.wflinfo.resolve()), env)
    else:
        print('[2/3] wflinfo sanity check skipped (--wflinfo not given)')

    print(f'  running {len(tests)} tests (timeout {args.timeout}s/test, '
          f'chunks of {args.chunk})')
    raw_log = (args.out / 'raw.log').open('wb')
    t0 = time.time()
    outcomes = {}
    chunks = [cmds[i:i + args.chunk] for i in range(0, len(cmds), args.chunk)]
    idx_base = 0
    for ci, chunk in enumerate(chunks):
        lines = ['#!/bin/sh', env_lines, f'cd {shlex.quote(str(piglit_root))}']
        for j, (name, cmd) in enumerate(chunk):
            idx = idx_base + j
            quoted = ' '.join(shlex.quote(a) for a in cmd)
            lines.append(f'echo "@@@T {idx} START"')
            lines.append(f'timeout -k 5 {args.timeout} {quoted} </dev/null 2>&1')
            lines.append(f'echo "@@@T {idx} EXIT $?"')
        script = '\n'.join(lines) + '\n'
        with tempfile.NamedTemporaryFile('w', suffix='.sh', delete=False) as tf:
            tf.write(script)
            host_script = tf.name
        try:
            r = subprocess.run(['sh', host_script], capture_output=True)
        finally:
            os.unlink(host_script)
        raw_log.write(r.stdout)
        raw_log.flush()

        # parse this chunk
        cur = None
        cur_results = []
        cur_out = []
        for line in r.stdout.splitlines():
            m = MARK_START.search(line)
            if m:
                cur = int(m.group(1))
                cur_results = []
                cur_out = []
                continue
            m = MARK_EXIT.search(line)
            if m and cur is not None and int(m.group(1)) == cur:
                code = int(m.group(2))
                name = cmds[cur][0]
                status = classify(code, cur_results, code in TIMEOUT_EXITS)
                outcomes[name] = {
                    'status': status,
                    'exit': code,
                    'subtests': {k: v for r_ in cur_results if 'subtest' in r_
                                 for k, v in r_['subtest'].items()},
                    'tail': b'\n'.join(cur_out[-8:]).decode(errors='replace')
                            if status in ('crash', 'timeout', 'fail', 'notrun') else '',
                }
                cur = None
                continue
            if cur is not None:
                cur_out.append(line)
                m = RESULT_LINE.search(line)
                if m:
                    try:
                        cur_results.append(json.loads(m.group(1)))
                    except json.JSONDecodeError:
                        pass
        idx_base += len(chunk)
        done = idx_base
        print(f'  chunk {ci + 1}/{len(chunks)} done ({done}/{len(cmds)}, '
              f'{time.time() - t0:.0f}s elapsed)')
    raw_log.close()

    print('[3/3] writing results')
    counts = {}
    for name, o in outcomes.items():
        counts[o['status']] = counts.get(o['status'], 0) + 1
    result_doc = {
        'backend': 'FluorateGL',
        'library': str(args.library.resolve()),
        'timeout': args.timeout,
        'elapsed_sec': round(time.time() - t0, 1),
        'totals': counts,
        'tests': outcomes,
    }
    (args.out / 'results.json').write_text(json.dumps(result_doc, indent=1, sort_keys=True))
    lines = [f"backend: {result_doc['backend']}  tests: {len(outcomes)}  "
             f"elapsed: {result_doc['elapsed_sec']}s"]
    lines.append('totals: ' + ', '.join(f'{k}={v}' for k, v in sorted(counts.items())))
    for status in ('crash', 'timeout', 'fail', 'missing', 'notrun', 'warn'):
        bad = sorted(n for n, o in outcomes.items() if o['status'] == status)
        if bad:
            lines.append(f'\n== {status} ({len(bad)}):')
            lines.extend(f'  {n}' for n in bad)
    (args.out / 'summary.txt').write_text('\n'.join(lines) + '\n')
    print('\n'.join(lines[:2]))
    print(f"results: {args.out / 'results.json'}")
    return 0


if __name__ == '__main__':
    sys.exit(main())

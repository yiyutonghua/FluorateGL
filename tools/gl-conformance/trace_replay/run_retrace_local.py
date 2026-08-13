#!/usr/bin/env python3
# Desktop Linux local runner for the trace replay matrix (single backend).
#
# Usage:
#   python3 tools/gl-conformance/trace_replay/run_retrace_local.py --all
#   python3 tools/gl-conformance/trace_replay/run_retrace_local.py --case OpenRA
#
# Fixtures are hydrated from the MobileGL trace fixture mirror through
# scripts/fetch-trace-fixture-lfs.sh (the repository only tracks LFS pointers).
import argparse
import json
import shutil
import subprocess
import sys
from pathlib import Path

from trace_cases import case_with_defaults, load_trace_cases

ROOT = Path(__file__).resolve().parents[3]
FIXTURES = ROOT / "tools" / "gl-conformance" / "trace_replay" / "fixtures"
SCRIPTS = ROOT / "tools" / "gl-conformance" / "trace_replay" / "scripts"
RESULT_ROOT = ROOT / ".trace-work" / "retrace-result"
SUMMARY_DIR = ROOT / ".trace-work" / "retrace-summary"
SUMMARY_FILE = "retrace-overview.txt"
DEFAULT_REPLAY_EXE = ROOT / "build-test" / "tools" / "trace_replay" / "fluorategl_trace_replay"
BACKEND = "DirectGLES"

CASES = load_trace_cases()


def safe_case(name):
    return "".join(ch if ch.isalnum() or ch in "._-" else "_" for ch in name)


def is_lfs_pointer(path):
    return path.exists() and path.read_bytes()[:80].startswith(b"version https://git-lfs.github.com/spec/v1")


def ensure_fixture(case):
    """Hydrates the case fixtures through the mirror fetch script when any file is
    missing or still an LFS pointer."""
    files = [FIXTURES / case["trace_archive"], FIXTURES / case["golden"]]
    if case.get("alternate_golden"):
        files.append(FIXTURES / case["alternate_golden"])
    missing = [str(f) for f in files if not f.exists() or is_lfs_pointer(f)]
    if not missing:
        return True
    print(f"--- hydrating fixtures for {case['name']}: {', '.join(missing)} ---", flush=True)
    command = ["bash", str(SCRIPTS / "fetch-trace-fixture-lfs.sh"), case["name"], str(FIXTURES)]
    result = subprocess.run(command, cwd=ROOT)
    return result.returncode == 0


def run_case(case, replay_exe, fixture_dir):
    result_dir = RESULT_ROOT / f"{safe_case(case['name'])}-{BACKEND}"
    output_dir = result_dir / "output"
    if output_dir.exists():
        shutil.rmtree(output_dir)
    output_dir.mkdir(parents=True)

    trace_archive = fixture_dir / case["trace_archive"]
    golden = fixture_dir / case["golden"]
    alternate = fixture_dir / case["alternate_golden"] if case.get("alternate_golden") else None
    if not trace_archive.exists() or is_lfs_pointer(trace_archive):
        return 2, f"trace archive missing or still an LFS pointer: {trace_archive}"
    if not golden.exists() or is_lfs_pointer(golden):
        return 2, f"golden image missing or still an LFS pointer: {golden}"
    if alternate is not None and (not alternate.exists() or is_lfs_pointer(alternate)):
        alternate = None

    extract_dir = output_dir / "input"
    extract_dir.mkdir(parents=True)
    subprocess.run(["tar", "-xzf", str(trace_archive)], cwd=extract_dir, check=True)
    trace_path = extract_dir / case["trace_file"]
    if not trace_path.exists():
        return 2, f"extracted trace was not found at {trace_path}"

    command = [
        str(replay_exe),
        "--trace", str(trace_path),
        "--golden", str(golden),
        "--diff", str(output_dir / f"{safe_case(case['name'])}-diff.png"),
        "--output", str(output_dir),
        "--backend", BACKEND,
        "--fluorategl-library", str(ROOT / "target" / "x86_64-unknown-linux-gnu" / "debug" / "libfluorategl.so"),
        "--target-call", str(case["target_call"]),
        "--width", str(case["width"]),
        "--height", str(case["height"]),
        "--ssim-threshold", str(case["ssim_threshold"]),
        "--crop-x", str(case["crop_x"]),
        "--crop-y", str(case["crop_y"]),
        "--crop-width", str(case["crop_width"]),
        "--crop-height", str(case["crop_height"]),
    ]
    if alternate is not None:
        command.insert(command.index("--target-call"), "--alternate-golden")
        command.insert(command.index("--target-call"), str(alternate))
    if case.get("coherent_as_flush"):
        command.append("--coherent-as-flush")
    result = subprocess.run(command, cwd=ROOT)
    return result.returncode, None


def render_summary():
    SUMMARY_DIR.mkdir(parents=True, exist_ok=True)
    results = []
    for result_dir in sorted(RESULT_ROOT.glob(f"*-{BACKEND}")):
        result_file = result_dir / "output" / "result.json"
        if result_file.exists():
            with result_file.open("r", encoding="utf-8") as f:
                results.append(json.load(f))
    lines = []
    lines.append(f"FluorateGL trace replay overview ({BACKEND})")
    lines.append(f"cases run: {len(results)}")
    lines.append("")
    lines.append(f"{'case':<60} {'status':<8} {'ssim':<10} {'threshold':<10} {'mismatch':<10}")
    lines.append("-" * 100)
    failed = 0
    for result in sorted(results, key=lambda r: r["tracePath"]):
        status = "PASS" if result["passed"] else "FAIL"
        if not result["passed"]:
            failed += 1
        case_label = Path(result["tracePath"]).name.replace(".tgz", "")
        lines.append(
            f"{case_label:<60} {status:<8} {result['ssim']:<10.6f} {result['ssimThreshold']:<10.2f} "
            f"{result['mismatchPixels']:<10}"
        )
    lines.append("")
    lines.append(f"failed: {failed}")
    summary = "\n".join(lines) + "\n"
    (SUMMARY_DIR / SUMMARY_FILE).write_text(summary, encoding="utf-8")
    print(summary)
    return failed


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("--case", action="append", dest="cases", help="Case name to run; may be repeated.")
    parser.add_argument("--all", action="store_true", help="Run every case in the CI matrix.")
    parser.add_argument("--replay-exe", default=str(DEFAULT_REPLAY_EXE), help="Path to fluorategl_trace_replay.")
    parser.add_argument("--fixture-dir", default=str(FIXTURES), help="Fixture directory.")
    parser.add_argument("--keep-results", action="store_true", help="Do not clear the previous result root.")
    return parser.parse_args()


def main():
    args = parse_args()
    replay_exe = Path(args.replay_exe)
    fixture_dir = Path(args.fixture_dir)
    if not replay_exe.exists():
        print(f"replay executable not found: {replay_exe}", file=sys.stderr)
        return 2
    selected_names = set(args.cases or [])
    selected_cases = [case_with_defaults(case) for case in CASES if args.all or case["name"] in selected_names]
    if not selected_cases:
        print("No cases selected. Use --all or --case NAME.", file=sys.stderr)
        return 2
    if not args.keep_results and RESULT_ROOT.exists():
        shutil.rmtree(RESULT_ROOT)
    RESULT_ROOT.mkdir(parents=True, exist_ok=True)
    failures = 0
    for case in selected_cases:
        print(f"=== retrace: {case['name']} / {BACKEND} ===", flush=True)
        if not ensure_fixture(case):
            print(f"failed to hydrate fixtures for {case['name']}", file=sys.stderr)
            failures += 1
            continue
        rc, message = run_case(case, replay_exe, fixture_dir)
        if message:
            print(message, file=sys.stderr)
        if rc not in (0, 2):
            failures += 1
    failed_cases = render_summary()
    return 1 if (failures or failed_cases) else 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Parallel golden-corpus probe for `tests/cases/*.dream` via `dream run` (native C)."""
import os
import re
import signal
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

_ANSI = re.compile(r"\x1b\[[0-9;]*m")

root = Path(__file__).resolve().parents[1]
dream = root / "target/debug/dream"
cases = sorted((root / "tests/cases").glob("*.dream"))
workers = int(os.environ.get("PROBE_JOBS", "8"))

USAGE = """\
Usage: probe_test.py [case-stem ...]

  stems      optional filter (e.g. arithmetic regex_basics)
"""


def parse_args(argv):
    only = []
    for arg in argv:
        if arg in ("-h", "--help"):
            sys.stdout.write(USAGE)
            sys.exit(0)
        if arg.startswith("-"):
            sys.stderr.write(f"unknown flag {arg}\n{USAGE}")
            sys.exit(2)
        only.append(arg)
    return set(only) if only else None


def run_group(args, timeout):
    proc = subprocess.Popen(
        args,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        start_new_session=True,
        cwd=root,
    )
    try:
        out, err = proc.communicate(timeout=timeout)
        return proc.returncode, out or "", err or ""
    except subprocess.TimeoutExpired:
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        proc.wait()
        return -9, "timeout", ""


def run_output_body(out):
    # Program stdout only. Compiler diagnostics go to stderr; do not drop blank lines or
    # lines that happen to start with `error:` (e.g. `error: divide by zero`).
    return _ANSI.sub("", out).strip()


def one(f: Path):
    stem = f.stem
    err = f.with_suffix(".expected_error")
    exp = f.with_suffix(".expected")
    trap = f.with_suffix(".expected_trap")
    if err.exists():
        code, _out, _err = run_group([str(dream), str(f)], 25)
        if code == 0:
            return stem, "fail", "compile should fail"
        return stem, "ok", ""
    # Debug `cc -O0` of large `@json` units is slow; leave headroom for a cold
    # `libdream_rt.a` rebuild and a loaded machine.
    code, out, err = run_group([str(dream), "run", str(f)], 180)
    if trap.exists():
        if code == 0:
            return stem, "fail", "expected trap"
        return stem, "ok", ""
    if code != 0:
        tail = " | ".join((err or out or "").strip().splitlines()[-2:])
        return stem, "fail", f"run {code} {tail}"
    if exp.exists():
        want = exp.read_text().strip()
        got = run_output_body(out)
        if got != want:
            return stem, "fail", f"output mismatch got={got[:80]!r}"
    return stem, "ok", ""


def main():
    only = parse_args(sys.argv[1:])
    if not dream.is_file():
        sys.stderr.write(f"missing {dream}; build with `cargo build`\n")
        sys.exit(2)
    files = [p for p in cases if not only or p.stem in only]
    fails = []
    ok = 0
    with ThreadPoolExecutor(max_workers=workers) as ex:
        futs = {ex.submit(one, p): p for p in files}
        done = 0
        for fut in as_completed(futs):
            done += 1
            label, status, msg = fut.result()
            print(f"[{done}/{len(files)}] {label} {status}", flush=True)
            if status == "ok":
                ok += 1
            else:
                fails.append(f"{label}: {msg}")

    print(f"ok={ok} fail={len(fails)} total={len(files)}", flush=True)
    for line in sorted(fails):
        print(line)
    sys.exit(1 if fails else 0)


if __name__ == "__main__":
    main()

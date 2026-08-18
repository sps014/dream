#!/usr/bin/env python3
"""Parallel native-C corpus probe. Always runs every tests/cases/*.dream."""
import os
import signal
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

root = Path(__file__).resolve().parents[1]
dream = root / "target/debug/dream"
cases = sorted((root / "tests/cases").glob("*.dream"))
only = set(sys.argv[1:]) if len(sys.argv) > 1 else None
workers = int(os.environ.get("PROBE_JOBS", "8"))


def run_group(args, timeout):
    proc = subprocess.Popen(
        args,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        start_new_session=True,
        cwd=root,
    )
    try:
        out, _ = proc.communicate(timeout=timeout)
        return proc.returncode, out or ""
    except subprocess.TimeoutExpired:
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        proc.wait()
        return -9, "timeout"


def one(f: Path):
    stem = f.stem
    err = f.with_suffix(".expected_error")
    exp = f.with_suffix(".expected")
    trap = f.with_suffix(".expected_trap")
    if err.exists():
        code, out = run_group([str(dream), "--native-c", str(f)], 25)
        if code == 0:
            return stem, "fail", "compile should fail"
        return stem, "ok", ""
    # Debug `cc -O0` of large `@json` units is ~20s serial / ~45s at 8-way; leave headroom
    # for a cold `libdream_rt.a` rebuild and a loaded machine.
    code, out = run_group([str(dream), "--native-c", "run", str(f)], 180)
    if trap.exists():
        if code == 0:
            return stem, "fail", "expected trap"
        return stem, "ok", ""
    if code != 0:
        tail = " | ".join((out or "").strip().splitlines()[-2:])
        return stem, "fail", f"run {code} {tail}"
    if exp.exists():
        want = exp.read_text().strip()
        # stdout is mixed with compiler logs; take last non-empty run output after "Executing"
        body = out
        if "Executing native C" in out:
            body = out.split("Executing native C", 1)[-1]
        got = "\n".join(
            ln for ln in body.splitlines() if not ln.startswith("ERROR") and not ln.startswith("INFO")
        ).strip()
        if got != want:
            return stem, "fail", f"output mismatch got={got[:80]!r}"
    return stem, "ok", ""


todo = [p for p in cases if not only or p.stem in only]
fails = []
ok = 0
with ThreadPoolExecutor(max_workers=workers) as ex:
    futs = {ex.submit(one, p): p for p in todo}
    done = 0
    for fut in as_completed(futs):
        done += 1
        stem, status, msg = fut.result()
        print(f"[{done}/{len(todo)}] {stem} {status}", flush=True)
        if status == "ok":
            ok += 1
        else:
            fails.append(f"{stem}: {msg}")

print(f"ok={ok} fail={len(fails)} total={len(todo)}", flush=True)
for line in sorted(fails):
    print(line)
sys.exit(1 if fails else 0)

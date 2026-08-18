#!/usr/bin/env python3
"""Parallel golden-corpus probe for `tests/cases/*.dream`.

Default backend is native C (`dream --native-c`). Pass `--wasm` to also run each
case under Wasmtime (`dream run`).
"""
import os
import signal
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

root = Path(__file__).resolve().parents[1]
dream = root / "target/debug/dream"
cases = sorted((root / "tests/cases").glob("*.dream"))
workers = int(os.environ.get("PROBE_JOBS", "8"))

USAGE = """\
Usage: probe_test.py [--wasm] [case-stem ...]

  (default)  native C backend
  --wasm     also run the same cases with the wasm / Wasmtime backend
  stems      optional filter (e.g. arithmetic regex_basics)
"""


def parse_args(argv):
    backends = ["c"]
    only = []
    for arg in argv:
        if arg in ("-h", "--help"):
            sys.stdout.write(USAGE)
            sys.exit(0)
        if arg == "--wasm":
            if "wasm" not in backends:
                backends.append("wasm")
            continue
        if arg.startswith("-"):
            sys.stderr.write(f"unknown flag {arg}\n{USAGE}")
            sys.exit(2)
        only.append(arg)
    return backends, (set(only) if only else None)


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


def dream_cmd(backend, *rest):
    cmd = [str(dream)]
    if backend == "c":
        cmd.append("--native-c")
    cmd.extend(rest)
    return cmd


def run_output_body(out, backend):
    marker = "Executing native C" if backend == "c" else "Executing via Wasmtime"
    body = out.split(marker, 1)[-1] if marker in out else out
    return "\n".join(
        ln
        for ln in body.splitlines()
        if not ln.startswith("ERROR") and not ln.startswith("INFO")
    ).strip()


def one(f: Path, backend: str):
    stem = f.stem
    label = f"{stem}[{backend}]"
    err = f.with_suffix(".expected_error")
    exp = f.with_suffix(".expected")
    trap = f.with_suffix(".expected_trap")
    if err.exists():
        code, _out = run_group(dream_cmd(backend, str(f)), 25)
        if code == 0:
            return label, "fail", "compile should fail"
        return label, "ok", ""
    # Debug `cc -O0` of large `@json` units is ~20s serial / ~45s at 8-way; leave headroom
    # for a cold `libdream_rt.a` rebuild and a loaded machine. Wasmtime is usually faster.
    timeout = 180 if backend == "c" else 60
    code, out = run_group(dream_cmd(backend, "run", str(f)), timeout)
    if trap.exists():
        if code == 0:
            return label, "fail", "expected trap"
        return label, "ok", ""
    if code != 0:
        tail = " | ".join((out or "").strip().splitlines()[-2:])
        return label, "fail", f"run {code} {tail}"
    if exp.exists():
        want = exp.read_text().strip()
        got = run_output_body(out, backend)
        if got != want:
            return label, "fail", f"output mismatch got={got[:80]!r}"
    return label, "ok", ""


def main():
    backends, only = parse_args(sys.argv[1:])
    if not dream.is_file():
        sys.stderr.write(f"missing {dream}; build with `cargo build`\n")
        sys.exit(2)
    files = [p for p in cases if not only or p.stem in only]
    todo = [(p, b) for p in files for b in backends]
    fails = []
    ok = 0
    with ThreadPoolExecutor(max_workers=workers) as ex:
        futs = {ex.submit(one, p, b): (p, b) for p, b in todo}
        done = 0
        for fut in as_completed(futs):
            done += 1
            label, status, msg = fut.result()
            print(f"[{done}/{len(todo)}] {label} {status}", flush=True)
            if status == "ok":
                ok += 1
            else:
                fails.append(f"{label}: {msg}")

    print(f"ok={ok} fail={len(fails)} total={len(todo)} backends={'+'.join(backends)}", flush=True)
    for line in sorted(fails):
        print(line)
    sys.exit(1 if fails else 0)


if __name__ == "__main__":
    main()

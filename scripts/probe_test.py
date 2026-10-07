#!/usr/bin/env python3
"""Parallel golden-corpus probe for `tests/cases/*.dream`.

Default: `dream run` (native).
`--parity`: run both targets and compare executed-program stdout directly.
`--node`: compile `--wasm` to `target/probe-wasm/{stem}/` and run via Node + `runtime/dream.js`.
"""
import json
import os
import re
import signal
import struct
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

_ANSI = re.compile(r"\x1b\[[0-9;]*m")
_LEAK = re.compile(r"\[dream\] leak check: live=(\d+)")

root = Path(__file__).resolve().parents[1]
# DREAM_PROBE_BIN probes another build, e.g. a staged release archive's `dream`.
dream_name = "dream.exe" if os.name == "nt" else "dream"
dream = Path(os.environ.get("DREAM_PROBE_BIN") or root / "target/debug" / dream_name)
cases = sorted((root / "tests/cases").glob("*.dream"))
workers = int(os.environ.get("PROBE_JOBS", "8"))
dream_js = root / "runtime" / "dream.js"

USAGE = """\
Usage: probe_test.py [--node | --parity] [--release] [--debug-info] [case-stem ...]

  --node          compile wasm32 and run with Node (not native `dream run`)
  --parity        run both targets and compare executed-program stdout directly
  --release       optimized build
  --debug-info    include debugger information
  stems      optional filter (e.g. arithmetic task_basic)
"""

# Hosts that exist natively only (files and interactive stdin).
_NODE_SKIP_PREFIXES = (
    "file_",
    "dir_",
    "sqlite",
)
_NODE_SKIP_STEMS = {
    # This asserts that the native process has PATH; guests expose an empty environment.
    "c_option_getenv",
    "console_read_line",
    "process_args_basic",
    "process_usage",
    # Asserts `System.platform() == Platform.Native`; true only on the native host.
    "platform_basic",
}


BUILD_FLAGS = []


def parse_args(argv):
    only = []
    node = False
    parity = False
    it = iter(argv)
    for arg in it:
        if arg in ("-h", "--help"):
            sys.stdout.write(USAGE)
            sys.exit(0)
        if arg == "--node":
            node = True
            continue
        if arg == "--parity":
            parity = True
            continue
        if arg == "--release":
            BUILD_FLAGS.append("--release")
            continue
        if arg == "--debug-info":
            BUILD_FLAGS.append("-g")
            continue
        if arg.startswith("-"):
            sys.stderr.write(f"unknown flag {arg}\n{USAGE}")
            sys.exit(2)
        only.append(arg)
    if node and parity:
        sys.stderr.write("--node and --parity are mutually exclusive\n")
        sys.exit(2)
    return node, parity, (set(only) if only else None)


def run_group(args, timeout, stdin=None, env=None):
    proc_env = os.environ.copy()
    if env:
        proc_env.update(env)
    proc = subprocess.Popen(
        args,
        stdin=subprocess.PIPE if stdin is not None else None,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        encoding="utf-8",
        errors="replace",
        start_new_session=True,
        cwd=root,
        env=proc_env,
    )
    try:
        out, err = proc.communicate(input=stdin, timeout=timeout)
        return proc.returncode, out or "", err or ""
    except subprocess.TimeoutExpired:
        try:
            if os.name == "nt":
                subprocess.run(["taskkill", "/F", "/T", "/PID", str(proc.pid)],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                               check=False)
            else:
                os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        proc.wait()
        return -9, "timeout", ""


def leak_failure(*streams):
    """A guest ARC leak, if the runtime's leak checker reported one.

    The check prints to stderr and `dream run` passes it through without inspecting it (only the
    captured-output path in `src/execution/native` fails on it), so the corpus has to look for
    itself. A non-zero `live` count is a failure the same as a wrong stdout: every case is expected
    to end holding nothing.
    """
    for s in streams:
        m = _LEAK.search(s or "")
        if m and m.group(1) != "0":
            detail = _ANSI.sub("", s).strip()
            return f"leak live={m.group(1)} stderr={detail[-2000:]!r}"
    return None


def missing_needle(expected: Path, *streams):
    """The first non-blank line of `expected` that appears in none of `streams`, if any.

    `.expected_error` and `.expected_trap` list fragments the diagnostics or the trap output must
    contain, one per line, rather than the exact text.
    """
    hay = _ANSI.sub("", "\n".join(s or "" for s in streams))
    for line in expected.read_text(encoding="utf-8").splitlines():
        needle = line.strip()
        if needle and needle not in hay:
            return needle
    return None


def run_output_body(out):
    # Program stdout only. Compiler diagnostics go to stderr; do not drop blank lines or
    # lines that happen to start with `error:` (e.g. `error: divide by zero`).
    return _ANSI.sub("", out).strip()


def js_string(s):
    return json.dumps(s)


def file_url(p: Path) -> str:
    return p.resolve().as_uri()


def node_skip_reason(stem):
    if stem in _NODE_SKIP_STEMS:
        return "native-only host"
    for p in _NODE_SKIP_PREFIXES:
        if stem.startswith(p) or stem == p.rstrip("_"):
            return "native-only host"
    return None


def one_node(f: Path, stdout_record=None):
    stem = f.stem
    err = f.with_suffix(".expected_error")
    exp = f.with_suffix(".expected")
    trap = f.with_suffix(".expected_trap")
    dest_dir = root / "target" / "probe-wasm" / stem
    dest_dir.mkdir(parents=True, exist_ok=True)
    wat = dest_dir / f"{stem}.wat"
    compile_cmd = [
        str(dream),
        *BUILD_FLAGS,
        "--wasm",
        "-o",
        str(wat),
        str(f),
    ]
    if err.exists():
        code, out, err_txt = run_group(compile_cmd, 180)
        if code == 0:
            return stem, "fail", "compile should fail"
        if code == -9:
            return stem, "fail", "compile timed out"
        needle = missing_needle(err, out, err_txt)
        if needle:
            return stem, "fail", f"diagnostics missing {needle!r}: {_ANSI.sub('', err_txt)[-2000:]!r}"
        return stem, "ok", ""
    skip = node_skip_reason(stem)
    if skip:
        return stem, "skip", skip

    code, out, err_txt = run_group(compile_cmd, 180)
    if code != 0:
        tail = " | ".join((err_txt or out or "").strip().splitlines()[-2:])
        return stem, "fail", f"compile {code} {tail}"

    wasm = wat.with_suffix(".wasm")
    if not wasm.is_file():
        return stem, "fail", "missing .wasm"
    js_url = js_string(file_url(dream_js))
    wasm_url = js_string(str(wasm.resolve()))
    runner = dest_dir / f"{stem}_run.mjs"
    runner.write_text(
        f"""import {{ run }} from {js_url};
const timer = setTimeout(() => {{ console.error('probe --node timeout'); process.exit(2); }}, 25000);
try {{
  await run({wasm_url}, {{ stdout: (s) => process.stdout.write(s) }});
}} finally {{
  clearTimeout(timer);
}}
""",
        encoding="utf-8",
    )
    code, out, err_txt = run_group(["node", str(runner)], 35)
    if code == -9 or (code == 2 and "probe --node timeout" in err_txt):
        return stem, "fail", "node timed out"
    if trap.exists():
        if code == 0:
            return stem, "fail", "expected trap"
        needle = missing_needle(trap, out, err_txt, f"exit code {code}")
        if needle:
            return stem, "fail", f"trap output missing {needle!r}: {_ANSI.sub('', err_txt)[-2000:]!r}"
        if stdout_record is not None:
            stdout_record.append(out)
        return stem, "ok", ""
    if code != 0:
        tail = " | ".join((err_txt or out or "").strip().splitlines()[-2:])
        return stem, "fail", f"node {code} {tail}"
    if exp.exists():
        want = exp.read_text(encoding="utf-8").strip()
        got = run_output_body(out)
        if got != want:
            detail = _ANSI.sub("", err_txt).strip()
            return stem, "fail", f"output mismatch got={got[:2000]!r} stderr={detail[-2000:]!r}"
    leak = leak_failure(err_txt, out)
    if leak:
        return stem, "fail", leak
    if stdout_record is not None:
        stdout_record.append(out)
    return stem, "ok", ""


def one(f: Path, stdout_record=None):
    stem = f.stem
    err = f.with_suffix(".expected_error")
    native_exp = f.with_suffix(".expected.native")
    exp = native_exp if struct.calcsize("P") == 8 and native_exp.exists() else f.with_suffix(".expected")
    trap = f.with_suffix(".expected_trap")
    if err.exists():
        code, out, err_txt = run_group([str(dream), *BUILD_FLAGS, str(f)], 180)
        if code == 0:
            return stem, "fail", "compile should fail"
        if code == -9:
            return stem, "fail", "compile timed out"
        needle = missing_needle(err, out, err_txt)
        if needle:
            return stem, "fail", f"diagnostics missing {needle!r}: {_ANSI.sub('', err_txt)[-2000:]!r}"
        return stem, "ok", ""

    cmd = [str(dream), *BUILD_FLAGS, "run", str(f)]
    stdin = None
    env = None
    if stem == "console_read_line":
        stdin = "hello-line\n"
    elif stem == "process_args_basic":
        cmd.extend(["--", "alpha", "beta"])
    # Debug `cc -O0` of large `@json` units is slow; leave headroom for a cold
    # `libdream_rt.a` rebuild and a loaded machine.
    code, out, err = run_group(cmd, 180, stdin=stdin, env=env)
    if code == -9:
        return stem, "fail", "run timed out"

    if trap.exists():
        if code == 0:
            return stem, "fail", "expected trap"
        needle = missing_needle(trap, out, err, f"exit code {code}")
        if needle:
            return stem, "fail", f"trap output missing {needle!r}: {_ANSI.sub('', err)[-2000:]!r}"
        if stdout_record is not None:
            stdout_record.append(out)
        return stem, "ok", ""
    if code != 0:
        tail = " | ".join((err or out or "").strip().splitlines()[-2:])
        return stem, "fail", f"run {code} {tail}"
    if exp.exists():
        want = exp.read_text(encoding="utf-8").strip()
        got = run_output_body(out)
        if got != want:
            detail = _ANSI.sub("", err).strip()
            return stem, "fail", f"output mismatch got={got[:2000]!r} stderr={detail[-2000:]!r}"
    leak = leak_failure(err, out)
    if leak:
        return stem, "fail", leak
    if stdout_record is not None:
        stdout_record.append(out)
    return stem, "ok", ""


def one_parity(f: Path):
    # Compile diagnostics are target-specific; every executed guest has comparable stdout.
    if not f.with_suffix(".expected_error").exists():
        reason = node_skip_reason(f.stem)
        if reason:
            return f.stem, "skip", reason
        if f.with_suffix(".expected.native").exists():
            return f.stem, "skip", "documented target-width output"
    native_stdout = []
    wasm_stdout = []
    native = one(f, native_stdout)
    if native[1] != "ok":
        return native
    wasm = one_node(f, wasm_stdout)
    if wasm[1] != "ok":
        return wasm
    if native_stdout != wasm_stdout:
        return f.stem, "fail", f"native/wasm stdout differ: native={native_stdout!r} wasm={wasm_stdout!r}"
    return f.stem, "ok", ""


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    node, parity, only = parse_args(sys.argv[1:])
    if not dream.is_file():
        sys.stderr.write(f"missing {dream}; build with `cargo build`\n")
        sys.exit(2)
    files = [p for p in cases if not only or p.stem in only]
    fails = []
    ok = 0
    skipped = 0
    run_one = one_parity if parity else one_node if node else one
    with ThreadPoolExecutor(max_workers=workers) as ex:
        futs = {ex.submit(run_one, p): p for p in files}
        done = 0
        for fut in as_completed(futs):
            done += 1
            label, status, msg = fut.result()
            extra = f" {msg}" if status == "skip" and msg else ""
            print(f"[{done}/{len(files)}] {label} {status}{extra}", flush=True)
            if status == "ok":
                ok += 1
            elif status == "skip":
                skipped += 1
            else:
                fails.append(f"{label}: {msg}")

    print(
        f"ok={ok} skip={skipped} fail={len(fails)} total={len(files)}",
        flush=True,
    )
    for line in sorted(fails):
        print(line)
    sys.exit(1 if fails else 0)


if __name__ == "__main__":
    main()

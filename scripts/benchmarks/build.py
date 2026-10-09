#!/usr/bin/env python3
"""Measure isolated cold, edited, and unchanged builds with LLVM subprocess traces."""
import argparse
import collections
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import time

from benchmarks.process import dream_name, peak_working_set, with_exe

ROOT = Path(__file__).resolve().parents[2]
TOOLS = ["clang", "opt", "llc", "llvm-link", "llvm-dis", "llvm-ar", "llvm-profdata", "llvm-rc", "wasm-ld", "cc"]
WRAPPER = r'''#!/usr/bin/env python3
import json, os, subprocess, sys, time
from pathlib import Path
name = os.environ.get("DREAM_BENCH_TOOL") or Path(sys.argv[0]).name
if name.endswith(".exe"):
    name = name[:-4]
suffix = ".exe" if os.name == "nt" and name != "cc" else ""
real = Path(os.environ["DREAM_BENCH_CC_REAL"]) if name == "cc" else Path(os.environ["DREAM_BENCH_LLVM_REAL"]) / (name + suffix)
started = time.monotonic()
result = subprocess.run([str(real), *sys.argv[1:]])
line = json.dumps({"tool": name, "args": sys.argv[1:], "seconds": time.monotonic() - started, "exit": result.returncode}) + "\n"
path = os.environ["DREAM_BENCH_TRACE"]
with open(path, "a", encoding="utf-8") as log:
    if os.name == "nt":
        lock = path + ".lock"
        while True:
            try:
                fd = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_RDWR)
                break
            except FileExistsError:
                time.sleep(0.01)
        try:
            log.write(line)
            log.flush()
        finally:
            os.close(fd)
            os.remove(lock)
    else:
        import fcntl
        fcntl.flock(log, fcntl.LOCK_EX)
        log.write(line)
sys.exit(result.returncode)
'''
# Dream invokes `clang.exe` by name, so each Windows shim is a real PE that records its own
# filename and then runs the Python tracer above.
STUB = r'''
#define WIN32_LEAN_AND_MEAN
#include <stdio.h>
#include <windows.h>
#include <wchar.h>

static wchar_t python[32768];
static wchar_t script[32768];
static wchar_t cmdline[32768];

static void tool_name(wchar_t *out, DWORD cap) {
    wchar_t path[MAX_PATH];
    DWORD n = GetModuleFileNameW(NULL, path, MAX_PATH);
    wchar_t *base = path;
    if (!n || n >= MAX_PATH) { out[0] = 0; return; }
    for (wchar_t *p = path; *p; ++p)
        if (*p == L'\\' || *p == L'/') base = p + 1;
    wcsncpy(out, base, cap - 1);
    out[cap - 1] = 0;
    size_t len = wcslen(out);
    if (len > 4) {
        wchar_t *ext = out + len - 4;
        if (ext[0] == L'.' && (ext[1] == L'e' || ext[1] == L'E') &&
            (ext[2] == L'x' || ext[2] == L'X') && (ext[3] == L'e' || ext[3] == L'E'))
            *ext = 0;
    }
}

int main(void) {
    wchar_t tool[64];
    wchar_t *full = GetCommandLineW();
    wchar_t *rest = full;
    tool_name(tool, 64);
    if (!GetEnvironmentVariableW(L"DREAM_BENCH_PYTHON", python, 32768) ||
        !GetEnvironmentVariableW(L"DREAM_BENCH_WRAPPER", script, 32768))
        return 127;
    SetEnvironmentVariableW(L"DREAM_BENCH_TOOL", tool);
    if (*rest == L'"') {
        ++rest;
        while (*rest && *rest != L'"') ++rest;
        if (*rest == L'"') ++rest;
    } else {
        while (*rest && *rest != L' ' && *rest != L'\t') ++rest;
    }
    _snwprintf(cmdline, 32768, L"\"%s\" \"%s\"%s", python, script, rest);
    STARTUPINFOW si;
    PROCESS_INFORMATION pi;
    ZeroMemory(&si, sizeof si);
    si.cb = sizeof si;
    if (!CreateProcessW(python, cmdline, NULL, NULL, TRUE, 0, NULL, NULL, &si, &pi))
        return 127;
    WaitForSingleObject(pi.hProcess, INFINITE);
    DWORD code = 1;
    GetExitCodeProcess(pi.hProcess, &code);
    CloseHandle(pi.hThread);
    CloseHandle(pi.hProcess);
    return (int)code;
}
'''
SOURCE = '''import system;
class Node {
 public value: int;
 public constructor(value: int) { this.value = value; }
}
fun main() { let node = Node(42); System.println(node.value); }
'''


def install_wrappers(work: Path, llvm: Path) -> Path:
    wrappers = work / "llvm"
    wrappers.mkdir()
    if os.name != "nt":
        for tool in TOOLS:
            path = wrappers / tool
            path.write_text(WRAPPER)
            path.chmod(0o755)
        return wrappers
    script = work / "llvm-wrapper.py"
    script.write_text(WRAPPER, encoding="utf-8", newline="\n")
    source = work / "llvm-stub.c"
    source.write_text(STUB, encoding="utf-8", newline="\n")
    stub = work / "llvm-stub.exe"
    compiled = subprocess.run(
        [str(with_exe(llvm / "clang")), "-O2", "-o", str(stub), str(source)],
        capture_output=True, text=True, encoding="utf-8", errors="replace")
    if compiled.returncode:
        raise RuntimeError(compiled.stderr or compiled.stdout or "could not compile the LLVM tracer")
    for tool in TOOLS:
        shutil.copy2(stub, wrappers / f"{tool}.exe")
    return wrappers


def run_build(binary, source, output, flags, env, trace, log, timing):
    trace.write_text("", encoding="utf-8")
    command = [str(binary), "-v", *flags, str(source), "-o", str(output)]
    started = time.monotonic()
    if os.name == "nt":
        proc = subprocess.Popen(command, cwd=ROOT, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                text=True, encoding="utf-8", errors="replace")
        captured, _ = proc.communicate()
        seconds = time.monotonic() - started
        log.write_text(captured, encoding="utf-8")
        if proc.returncode:
            raise RuntimeError(captured)
        rss = peak_working_set(proc)
    else:
        clock_args = ["-l"] if platform.system() == "Darwin" else ["-v"]
        result = subprocess.run(["/usr/bin/time", *clock_args, "-o", str(timing), *command],
                                cwd=ROOT, env=env, capture_output=True, text=True)
        seconds = time.monotonic() - started
        captured = result.stdout + result.stderr
        log.write_text(captured)
        if result.returncode:
            raise RuntimeError(captured)
        rss = None
        for line in timing.read_text().splitlines():
            if "maximum resident set size" in line:
                rss = int(line.split()[0])
            elif "Maximum resident set size (kbytes):" in line:
                rss = int(line.rsplit(":", 1)[1]) * 1024
    calls = [json.loads(line) for line in trace.read_text(encoding="utf-8").splitlines()]
    phase_seconds = {}
    factors = {"s": 1, "ms": .001, "µs": .000001, "ns": .000000001}
    for line in captured.splitlines():
        phases = re.findall(r'compile_phase\{phase="([^"]+)"\}', line)
        cost = re.search(r'close time.busy=([\d.]+)(s|ms|µs|ns)', line)
        if phases and cost and ':function_passes{' not in line and ':compile_tool{' not in line:
            phase_seconds[phases[-1]] = phase_seconds.get(phases[-1], 0) + float(cost[1]) * factors[cost[2]]
    return {"seconds": seconds, "max_rss_bytes": rss,
            "phase_seconds": phase_seconds,
            "subprocesses": dict(collections.Counter(call["tool"] for call in calls)),
            "subprocess_seconds": sum(call["seconds"] for call in calls),
            "trace": str(trace), "compiler_log": str(log)}


def file_digest(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def runtime_digest(root):
    digest = hashlib.sha256()
    for path in sorted(root.rglob("*")):
        if path.is_file():
            name = path.relative_to(root).as_posix().encode()
            digest.update(len(name).to_bytes(8, "little"))
            digest.update(name)
            digest.update(bytes.fromhex(file_digest(path)))
    return digest.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target" / "debug" / dream_name())
    parser.add_argument("--llvm", type=Path, required=True)
    parser.add_argument("--runtime", type=Path, default=ROOT / "crates/dream-mir/src/runtime/c")
    parser.add_argument("--repeat", type=int, default=3)
    parser.add_argument("--functions", type=int, default=0, help="add independently callable ARC functions")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.repeat < 1 or args.functions < 0:
        parser.error("repeat must be positive and functions must be nonnegative")
    source_text = SOURCE
    if args.functions:
        declarations = "\n".join(
            f"fun work_{i}(value: int): int {{ let node = Node(value); return node.value * 2 + 1; }}"
            for i in range(args.functions)
        )
        calls = "\n".join(f"total += work_{i}({i});" for i in range(args.functions))
        source_text = SOURCE.split("fun main()")[0] + declarations + (
            "\nfun main() { let node = Node(42); let total = node.value;\n"
            + calls + "\nSystem.println(total); }\n"
        )
    metadata = {
        "platform": platform.platform(),
        "binary": str(with_exe(args.binary).resolve()),
        "binary_sha256": file_digest(with_exe(args.binary).resolve()),
        "runtime_sha256": runtime_digest(args.runtime.resolve()),
        "source_sha256": hashlib.sha256(source_text.encode()).hexdigest(),
        "llvm": str(args.llvm.resolve()),
        "functions": args.functions,
    }
    destination = args.output.resolve()
    destination.parent.mkdir(parents=True, exist_ok=True)
    traces = destination.with_suffix(".traces")
    traces.mkdir(exist_ok=True)
    rows = []
    with tempfile.TemporaryDirectory(prefix="dream-build-bench-") as directory:
        work = Path(directory)
        wrappers = install_wrappers(work, args.llvm.resolve())
        cc = shutil.which("cc") or shutil.which("clang") or shutil.which("clang.exe") or ""
        for profile, flags in [("Debug", []), ("Debug-g", ["-g"]), ("Release", ["--release"])]:
            for repeat in range(args.repeat):
                project = work / f"{profile}-{repeat}"
                project.mkdir()
                source = project / "main.dream"
                output = project / "out/main.ll"
                source.write_text(source_text)
                env = dict(os.environ, DREAM_PREFIX=str(project / "prefix"), DREAM_LLVM=str(wrappers),
                           DREAM_RUNTIME_C=str(args.runtime.resolve()), DREAM_BENCH_LLVM_REAL=str(args.llvm.resolve()),
                           DREAM_BENCH_CC_REAL=cc, DREAM_CC=str(wrappers / ("cc.exe" if os.name == "nt" else "cc")))
                if os.name == "nt":
                    env["DREAM_BENCH_PYTHON"] = sys.executable
                    env["DREAM_BENCH_WRAPPER"] = str(work / "llvm-wrapper.py")
                live_trace = project / "trace.jsonl"
                env["DREAM_BENCH_TRACE"] = str(live_trace)
                for phase in ["cold", "unchanged", "edited", "unchanged_after_edit"]:
                    if phase == "edited":
                        source.write_text(source_text.replace("Node(42)", "Node(43)"))
                    key = f"{profile}-{repeat}-{phase}"
                    trace = traces / f"{key}.jsonl"
                    row = run_build(with_exe(args.binary).resolve(), source, output, flags, env, live_trace,
                                    traces / f"{key}.log", traces / f"{key}.time")
                    shutil.copyfile(live_trace, trace)
                    row["trace"] = str(trace)
                    rows.append(dict(profile=profile, repeat=repeat, phase=phase, **row))
                    destination.write_text(json.dumps(dict(metadata, rows=rows), indent=2) + "\n")
                    print(f"{key}: {row['seconds']:.3f}s {row['subprocesses']}", flush=True)

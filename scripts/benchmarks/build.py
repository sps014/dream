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
STDLIB_SOURCE = '''import system;
import system.json;
import system.text;

@json
class Developer {
    public name: string;
    public languages: string[];

    public constructor(name: string, languages: string[]) {
        this.name = name;
        this.languages = languages;
    }
}

fun main() {
    let developers = List<Developer>();
    developers.push(Developer("Ada", ["Dream", "TypeScript"]));
    developers.push(Developer("Grace", ["COBOL"]));
    let counts = Map<string, int>();
    let digits = Regex.compile("\\\\d+", RegexFlags.Global).unwrap();
    for (let developer in developers) {
        let text = Json.serialize(developer);
        counts[developer.name] = text.length;
        System.println(text);
    }
    let parsed = Json.deserialize<JsonValue>("{\\"seed\\": 42}").unwrap_or(JsonValue.null());
    System.println(parsed.get("seed").unwrap_or(JsonValue.null()).as_int().unwrap_or(0));
    System.println(digits.test("abc123"));
    System.println(counts.length);
}
'''

def generated_source(functions):
    """Mirrors `scripts/benchmarks/go_baseline.py`: one allocation, one loop and a branch per function."""
    parts = [SOURCE.split("fun main()")[0]]
    for i in range(functions):
        parts.append(f'''fun work_{i}(seed: int): int {{
    let node = Node(seed);
    let value = node.value;
    for (let i = 0; i < 32; i++) {{
        if (i % 3 == 0) {{
            value += i;
        }} else {{
            value -= i;
        }}
    }}
    return value;
}}
''')
    calls = "\n".join(f"    total += work_{i}({i});" for i in range(functions))
    parts.append(f"fun main() {{\n    let total = Node(42).value;\n{calls}\n    System.println(total);\n}}\n")
    return "".join(parts)


def workload(name):
    """Returns `(source, marker)`; the edited build changes the single `42` inside `marker`."""
    if name == "hello":
        return SOURCE, "Node(42)"
    if name == "stdlib":
        return STDLIB_SOURCE, '\\"seed\\": 42'
    if name == "webapi":
        return (ROOT / "sample/webapi/app.dream").read_text().replace("8000", "8042"), "8042"
    match = re.fullmatch(r"gen(\d+)", name)
    if match and int(match[1]) > 0:
        return generated_source(int(match[1])), "Node(42)"
    raise argparse.ArgumentTypeError(f"unknown workload {name!r}: use hello, genN, stdlib or webapi")


def install_wrappers(work: Path, llvm: Path) -> Path:
    # The pinned-tool lookup accepts an override only as the toolchain's own `bin/`; its siblings
    # carry the clang resource directory, compiler-rt and the WASI sysroot.
    wrappers = work / "llvm" / "bin"
    wrappers.mkdir(parents=True)
    for sibling in llvm.parent.iterdir():
        if sibling.name != "bin":
            (work / "llvm" / sibling.name).symlink_to(sibling, target_is_directory=sibling.is_dir())
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


def exclusive_phase_seconds(log):
    """Per-phase busy time with nested `compile_phase` spans subtracted from their parent."""
    factors = {"s": 1, "ms": .001, "µs": .000001, "ns": .000000001}
    inclusive = collections.Counter()
    nested = collections.Counter()
    for line in log.splitlines():
        phases = re.findall(r'compile_phase\{phase="([^"]+)"\}', line)
        cost = re.search(r'close time.busy=([\d.]+)(s|ms|µs|ns)', line)
        if not phases or not cost or ':function_passes{' in line or ':compile_tool{' in line:
            continue
        seconds = float(cost[1]) * factors[cost[2]]
        inclusive[phases[-1]] += seconds
        if len(phases) > 1:
            nested[phases[-2]] += seconds
    return {phase: max(total - nested[phase], 0.0) for phase, total in inclusive.items()}


def run_build(binary, source, output, flags, env, trace, log, timing, plain=False):
    trace.write_text("", encoding="utf-8")
    command = [str(binary), *([] if plain else ["-v"]), *flags, str(source), "-o", str(output)]
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
    return {"seconds": seconds, "max_rss_bytes": rss,
            "phase_seconds": exclusive_phase_seconds(captured),
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


PROFILES = {"Debug": [], "Debug-g": ["-g"], "Release": ["--release"]}
TARGETS = {"native": [], "wasm": ["--wasm"]}


def csv_choice(choices):
    def parse(value):
        items = [item for item in value.split(",") if item]
        unknown = [item for item in items if item not in choices]
        if unknown or not items:
            raise argparse.ArgumentTypeError(f"choose from {', '.join(choices)}")
        return items
    return parse


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target" / "debug" / dream_name())
    parser.add_argument("--llvm", type=Path, required=True)
    parser.add_argument("--runtime", type=Path, default=ROOT / "crates/dream-mir/src/runtime/c")
    parser.add_argument("--repeat", type=int, default=3)
    parser.add_argument("--workload", default="hello",
                        help="hello, genN (N generated functions), stdlib (json/regex/collections) or webapi")
    parser.add_argument("--profiles", type=csv_choice(PROFILES), default=list(PROFILES))
    parser.add_argument("--targets", type=csv_choice(TARGETS), default=["native"])
    parser.add_argument("--plain", action="store_true",
                        help="measure wall time only: no -v tracing and no subprocess wrappers")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.repeat < 1:
        parser.error("repeat must be positive")
    try:
        source_text, marker = workload(args.workload)
    except argparse.ArgumentTypeError as error:
        parser.error(str(error))
    edited_text = source_text.replace(marker, marker.replace("42", "43"), 1)
    metadata = {
        "platform": platform.platform(),
        "binary": str(with_exe(args.binary).resolve()),
        "binary_sha256": file_digest(with_exe(args.binary).resolve()),
        "runtime_sha256": runtime_digest(args.runtime.resolve()),
        "source_sha256": hashlib.sha256(source_text.encode()).hexdigest(),
        "llvm": str(args.llvm.resolve()),
        "workload": args.workload,
        "source_lines": source_text.count("\n"),
        "plain": args.plain,
    }
    destination = args.output.resolve()
    destination.parent.mkdir(parents=True, exist_ok=True)
    traces = destination.with_suffix(".traces")
    traces.mkdir(exist_ok=True)
    rows = []
    binary = with_exe(args.binary).resolve()
    with tempfile.TemporaryDirectory(prefix="dream-build-bench-") as directory:
        work = Path(directory)
        wrappers = install_wrappers(work, args.llvm.resolve())
        cc = shutil.which("cc") or shutil.which("clang") or shutil.which("clang.exe") or ""
        for target in args.targets:
            for profile in args.profiles:
                flags = PROFILES[profile] + TARGETS[target]
                for repeat in range(args.repeat):
                    project = work / f"{target}-{profile}-{repeat}"
                    project.mkdir()
                    source = project / "main.dream"
                    output = project / "out/main.ll"
                    source.write_text(source_text)
                    env = dict(os.environ, DREAM_PREFIX=str(project / "prefix"),
                               DREAM_RUNTIME_C=str(args.runtime.resolve()))
                    if args.plain:
                        env["DREAM_LLVM"] = str(args.llvm.resolve())
                    else:
                        env.update(DREAM_LLVM=str(wrappers), DREAM_BENCH_LLVM_REAL=str(args.llvm.resolve()),
                                   DREAM_BENCH_CC_REAL=cc,
                                   DREAM_CC=str(wrappers / ("cc.exe" if os.name == "nt" else "cc")))
                    if os.name == "nt":
                        env["DREAM_BENCH_PYTHON"] = sys.executable
                        env["DREAM_BENCH_WRAPPER"] = str(work / "llvm-wrapper.py")
                    live_trace = project / "trace.jsonl"
                    env["DREAM_BENCH_TRACE"] = str(live_trace)
                    for phase in ["cold", "unchanged", "edited", "unchanged_after_edit"]:
                        if phase == "edited":
                            source.write_text(edited_text)
                        key = f"{target}-{profile}-{repeat}-{phase}"
                        trace = traces / f"{key}.jsonl"
                        row = run_build(binary, source, output, flags, env, live_trace,
                                        traces / f"{key}.log", traces / f"{key}.time", args.plain)
                        shutil.copyfile(live_trace, trace)
                        row["trace"] = str(trace)
                        rows.append(dict(target=target, profile=profile, repeat=repeat, phase=phase, **row))
                        destination.write_text(json.dumps(dict(metadata, rows=rows), indent=2) + "\n")
                        print(f"{key}: {row['seconds']:.3f}s {row['subprocesses']}", flush=True)

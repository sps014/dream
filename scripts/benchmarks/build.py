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
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
WRAPPER = '''#!/usr/bin/env python3
import fcntl,json,os,subprocess,sys,time
from pathlib import Path
name=Path(sys.argv[0]).name
real=Path(os.environ["DREAM_BENCH_LLVM_REAL"])/name if name!="cc" else Path(os.environ["DREAM_BENCH_CC_REAL"])
t=time.monotonic()
result=subprocess.run([str(real),*sys.argv[1:]])
with open(os.environ["DREAM_BENCH_TRACE"],"a") as log:
 fcntl.flock(log,fcntl.LOCK_EX)
 log.write(json.dumps({"tool":name,"args":sys.argv[1:],"seconds":time.monotonic()-t,"exit":result.returncode})+"\\n")
sys.exit(result.returncode)
'''
SOURCE = '''import system;
class Node {
 public value: int;
 public constructor(value: int) { this.value = value; }
}
fun main() { let node = Node(42); System.println(node.value); }
'''


def run_build(binary, source, output, flags, env, trace, log, timing):
    trace.write_text("")
    clock_args = ["-l"] if platform.system() == "Darwin" else ["-v"]
    command = ["/usr/bin/time", *clock_args, "-o", str(timing), str(binary), "-v", *flags,
               str(source), "-o", str(output)]
    started = time.monotonic()
    result = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, text=True)
    seconds = time.monotonic() - started
    log.write_text(result.stdout + result.stderr)
    if result.returncode:
        raise RuntimeError(log.read_text())
    calls = [json.loads(line) for line in trace.read_text().splitlines()]
    phase_seconds = {}
    factors = {"s": 1, "ms": .001, "µs": .000001, "ns": .000000001}
    for line in (result.stdout + result.stderr).splitlines():
        phases = re.findall(r'compile_phase\{phase="([^"]+)"\}', line)
        cost = re.search(r'close time.busy=([\d.]+)(s|ms|µs|ns)', line)
        if phases and cost and ':function_passes{' not in line and ':compile_tool{' not in line:
            phase_seconds[phases[-1]] = phase_seconds.get(phases[-1], 0) + float(cost[1]) * factors[cost[2]]
    rss = None
    for line in timing.read_text().splitlines():
        if "maximum resident set size" in line:
            rss = int(line.split()[0])
        elif "Maximum resident set size (kbytes):" in line:
            rss = int(line.rsplit(":", 1)[1]) * 1024
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
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/dream")
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
        "binary": str(args.binary.resolve()),
        "binary_sha256": file_digest(args.binary.resolve()),
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
        wrappers = work / "llvm"
        wrappers.mkdir()
        for tool in ["clang", "opt", "llc", "llvm-link", "llvm-dis", "llvm-ar", "llvm-profdata", "llvm-rc", "wasm-ld", "cc"]:
            path = wrappers / tool
            path.write_text(WRAPPER)
            path.chmod(0o755)
        for profile, flags in [("Debug", []), ("Debug-g", ["-g"]), ("Release", ["--release"])]:
            for repeat in range(args.repeat):
                project = work / f"{profile}-{repeat}"
                project.mkdir()
                source = project / "main.dream"
                output = project / "out/main.ll"
                source.write_text(source_text)
                env = dict(os.environ, DREAM_PREFIX=str(project / "prefix"), DREAM_LLVM=str(wrappers),
                           DREAM_RUNTIME_C=str(args.runtime.resolve()), DREAM_BENCH_LLVM_REAL=str(args.llvm.resolve()),
                           DREAM_BENCH_CC_REAL=shutil.which("cc"), DREAM_CC=str(wrappers / "cc"))
                live_trace = project / "trace.jsonl"
                env["DREAM_BENCH_TRACE"] = str(live_trace)
                for phase in ["cold", "unchanged", "edited", "unchanged_after_edit"]:
                    if phase == "edited":
                        source.write_text(source_text.replace("Node(42)", "Node(43)"))
                    key = f"{profile}-{repeat}-{phase}"
                    trace = traces / f"{key}.jsonl"
                    row = run_build(args.binary.resolve(), source, output, flags, env, live_trace,
                                    traces / f"{key}.log", traces / f"{key}.time")
                    shutil.copyfile(live_trace, trace)
                    row["trace"] = str(trace)
                    rows.append(dict(profile=profile, repeat=repeat, phase=phase, **row))
                    destination.write_text(json.dumps(dict(metadata, rows=rows), indent=2) + "\n")
                    print(f"{key}: {row['seconds']:.3f}s {row['subprocesses']}", flush=True)



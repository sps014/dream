"""`scripts/bench.py profile`: build one benchmark fixture with symbols and record a CPU profile.

Picks the first available sampler: samply (any OS, opens the Firefox Profiler), perf on Linux,
xctrace's Time Profiler with full Xcode, else macOS's built-in `sample` call-tree report. Recording is never a timing reference; use the comparison
harness for numbers and this for where the time goes.
"""
from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path

from .process import dream_name, with_exe

ROOT = Path(__file__).resolve().parents[2]
SUITES = {
    "micro": ROOT / "tests/bench/microbenches.dream",
    "macro": ROOT / "tests/bench/macro/macrobenches.dream",
}


def xctrace_usable() -> bool:
    # The Command Line Tools ship an `xctrace` shim that only works with a full Xcode selected.
    return shutil.which("xctrace") is not None and subprocess.run(
        ["xctrace", "version"], capture_output=True).returncode == 0


def sampler(requested: str | None) -> str:
    if requested:
        if not shutil.which(requested) or (requested == "xctrace" and not xctrace_usable()):
            raise SystemExit(f"profile: {requested} not available")
        return requested
    for tool in ("samply", "perf"):
        if shutil.which(tool) and (tool != "perf" or sys.platform.startswith("linux")):
            return tool
    if sys.platform == "darwin":
        return "xctrace" if xctrace_usable() else "sample"
    raise SystemExit("profile: install samply (cargo install samply) or perf")


def record_with_sample(binary: Path, env: dict, report: Path) -> int:
    """macOS `sample` attaches to a running pid, so launch first and sample until exit."""
    proc = subprocess.Popen([str(binary)], env=env)
    sampler_proc = subprocess.Popen(["sample", str(proc.pid), "600", "1", "-mayDie", "-file", str(report)],
                                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    code = proc.wait()
    sampler_proc.wait()
    return code


def main() -> int:
    p = argparse.ArgumentParser(prog="bench.py profile", description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--suite", choices=sorted(SUITES), default="micro")
    p.add_argument("--benchmark", type=Path, help="fixture path (overrides --suite)")
    p.add_argument("--dream", default=str(ROOT / "target" / "release" / dream_name()))
    p.add_argument("--tool", choices=["samply", "perf", "xctrace", "sample"])
    p.add_argument("--passes", type=int, default=5, help="DREAM_BENCH_PASSES while recording")
    p.add_argument("--out", type=Path, default=ROOT / "tests/bench/out/profile")
    args = p.parse_args()

    fixture = (args.benchmark or SUITES[args.suite]).resolve()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    work_src = out / fixture.name
    work_src.write_bytes(fixture.read_bytes())
    dream = str(with_exe(Path(args.dream)).resolve())
    subprocess.run([dream, "--release", "-g", str(work_src)], cwd=out, check=True)
    binary = out / "target" / "release" / f"{fixture.stem}.bin"
    if not binary.is_file():
        raise SystemExit(f"profile: missing compiled artifact {binary}")

    env = dict(os.environ, DREAM_BENCH_PASSES=str(args.passes), DREAM_BENCH_SEED="1")
    tool = sampler(args.tool)
    if tool == "sample":
        report = out / "sample.txt"
        code = record_with_sample(binary, env, report)
        if code == 0:
            print(f"call-tree report: {report}", file=sys.stderr)
        return code
    if tool == "samply":
        cmd = ["samply", "record", "--save-only", "-o", str(out / "profile.json.gz"), str(binary)]
    elif tool == "perf":
        cmd = ["perf", "record", "-g", "--call-graph", "dwarf", "-o", str(out / "perf.data"), str(binary)]
    else:
        trace = out / "profile.trace"
        shutil.rmtree(trace, ignore_errors=True)
        cmd = ["xctrace", "record", "--template", "Time Profiler", "--output", str(trace),
               "--launch", "--", str(binary)]
    proc = subprocess.run(cmd, env=env)
    if proc.returncode == 0:
        print(f"profile written under {out}", file=sys.stderr)
    return proc.returncode

#!/usr/bin/env python3
"""Time one integer loop in Dream, Rust, and Go on this machine.

Each language that is installed is compiled at its release setting and run three times.
The median wall time is the row. Missing toolchains are reported and skipped, so a
laptop without Go still produces the rows it can. ubuntu-24.04 in CI is x86-64.
"""
from __future__ import annotations

import json
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HERE = ROOT / "tests" / "bench" / "compare"


def median_run(command: list[str], repeats: int = 3) -> float | None:
    samples = []
    for _ in range(repeats):
        start = time.perf_counter()
        result = subprocess.run(command, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        samples.append(time.perf_counter() - start)
        if result.returncode != 0:
            sys.stderr.write(result.stderr.decode(errors="replace"))
            return None
    return statistics.median(samples)


def main() -> int:
    rows = {"platform": sys.platform, "machine": subprocess.getoutput("uname -m").strip()}
    with tempfile.TemporaryDirectory(prefix="dream-compare-") as temporary:
        work = Path(temporary)
        dream = shutil.which("dream") or str(ROOT / "target" / "release" / "dream")
        if Path(dream).is_file():
            binary = work / "loop-dream"
            built = subprocess.run(
                [dream, str(HERE / "loop.dream"), "-o", str(binary)],
                stderr=subprocess.PIPE,
            )
            rows["dream_seconds"] = (
                median_run([str(binary)]) if built.returncode == 0 else None
            )
            if built.returncode != 0:
                sys.stderr.write(built.stderr.decode(errors="replace"))
        else:
            rows["dream_seconds"] = None
        if shutil.which("rustc"):
            binary = work / "loop-rust"
            built = subprocess.run(
                ["rustc", "-O", str(HERE / "loop.rs"), "-o", str(binary)],
                stderr=subprocess.PIPE,
            )
            rows["rust_seconds"] = median_run([str(binary)]) if built.returncode == 0 else None
        else:
            rows["rust_seconds"] = None
        if shutil.which("go"):
            binary = work / "loop-go"
            built = subprocess.run(
                ["go", "build", "-o", str(binary), str(HERE / "loop.go")],
                stderr=subprocess.PIPE,
            )
            rows["go_seconds"] = median_run([str(binary)]) if built.returncode == 0 else None
        else:
            rows["go_seconds"] = None
    json.dump(rows, sys.stdout, indent=2)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

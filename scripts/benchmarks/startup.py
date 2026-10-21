"""Native process launch-to-exit measurements, separate from in-process workload timing."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import random
import shutil
import statistics
import subprocess
import time

from .gate import digest, interval

ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for arm in ("baseline", "current"):
        parser.add_argument(f"--{arm}-dream", required=True, type=Path)
        parser.add_argument(f"--{arm}-runtime", required=True, type=Path)
    parser.add_argument("--llvm", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--rounds", type=int, default=20)
    parser.add_argument("--warmup", type=int, default=2)
    parser.add_argument("--passes", type=int, default=5)
    args = parser.parse_args()
    if args.rounds < 20 or args.warmup < 0 or args.passes < 5:
        raise ValueError("startup measurements require 20 rounds and five passes")
    args.out.mkdir(parents=True, exist_ok=False)
    llvm = args.llvm.resolve()
    tools = {}
    for name in ("clang", "opt", "llc", "llvm-link"):
        tool = llvm / (name + (".exe" if os.name == "nt" else ""))
        tools[name] = {"sha256": digest(tool), "version": subprocess.check_output([str(tool), "--version"], text=True)}
    identities = {}
    binaries = {}
    for arm in ("baseline", "current"):
        compiler = getattr(args, f"{arm}_dream").resolve()
        runtime = getattr(args, f"{arm}_runtime").resolve()
        environment = os.environ.copy()
        environment.pop("DREAM_RUNTIME_COUNTERS", None)
        environment.pop("DREAM_BENCH_COUNTERS", None)
        environment.pop("DREAM_SANITIZE", None)
        environment.pop("DREAM_ASAN", None)
        environment.pop("DREAM_NATIVE_SANITIZE", None)
        environment["DREAM_RUNTIME_C"] = str(runtime)
        directory = args.out / arm
        directory.mkdir()
        shutil.copy2(compiler, directory / compiler.name)
        shutil.copytree(runtime, directory / "runtime-c")
        environment["DREAM_RUNTIME_C"] = str((directory / "runtime-c").resolve())
        environment["DREAM_LLVM"] = str(llvm)
        compiler = (directory / compiler.name).resolve()
        identities[arm] = {
            "compiler_sha256": digest(compiler),
            "runtime": {str(p.relative_to(runtime)): digest(p)
                        for p in sorted(runtime.rglob("*")) if p.is_file()},
        }
        for fixture in sorted((ROOT / "tests/bench/startup").glob("*.dream")):
            source = directory / fixture.name
            shutil.copy2(fixture, source)
            output = source.with_suffix(".ll")
            command = [str(compiler), "--release", "--emit-opt-ir", "-o", str(output), str(source)]
            result = subprocess.run(command, env=environment, capture_output=True)
            (directory / (fixture.stem + ".compile.log")).write_bytes(result.stdout + result.stderr)
            result.check_returncode()
            artifact = output.with_suffix(".bin")
            binaries[arm, fixture.stem] = artifact
            for tool, flags in (("size", ["-m"] if platform.system() == "Darwin" else ["-A"]), ("nm", [])):
                executable = shutil.which(tool)
                if executable:
                    inspection = subprocess.run([executable, *flags, str(artifact)], capture_output=True)
                    if inspection.returncode == 0:
                        (directory / (fixture.stem + "." + tool + ".txt")).write_bytes(inspection.stdout)
                        tools[tool] = {"path": executable, "sha256": digest(executable)}
            identities[arm].setdefault("artifacts", {})[fixture.stem] = {
                "sha256": digest(artifact), "bytes": artifact.stat().st_size,
                "fixture_sha256": digest(fixture), "compile_command": command,
            }
    samples = []
    outputs = {}
    # A process launch includes loader, startup, workload and teardown. No phase attribution is
    # inferred from this timer; in-process suites measure workload throughput independently.
    environment = os.environ.copy()
    for name in ("DREAM_RUNTIME_COUNTERS", "DREAM_BENCH_COUNTERS", "DREAM_ASAN", "DREAM_SANITIZE", "DREAM_NATIVE_SANITIZE"):
        environment.pop(name, None)
    with (args.out / "raw.jsonl").open("w") as raw:
        for round_index in range(-args.warmup, args.rounds):
            order = ("baseline", "current") if round_index % 2 == 0 else ("current", "baseline")
            for name in sorted({name for _, name in binaries}):
                for arm in order:
                    for pass_index in range(args.passes):
                        started = time.perf_counter_ns()
                        result = subprocess.run([str(binaries[arm, name]), "first", "second"],
                                                env=environment, capture_output=True)
                        elapsed = time.perf_counter_ns() - started
                        result.check_returncode()
                        checksum = hashlib.sha256(result.stdout).hexdigest()
                        expected = outputs.setdefault(name, checksum)
                        if checksum != expected:
                            raise ValueError(f"output mismatch: {name}/{arm}")
                        if round_index >= 0:
                            row = {"arm": arm, "name": name, "round": round_index,
                                   "pass": pass_index, "ns": elapsed, "checksum": checksum}
                            raw.write(json.dumps(row) + "\n")
                            raw.flush()
                            samples.append(row)
    summary = {}
    rng = random.Random(0x5eed)
    for name in sorted(outputs):
        medians = {}
        for arm in ("baseline", "current"):
            medians[arm] = [statistics.median(s["ns"] for s in samples
                              if s["name"] == name and s["arm"] == arm and s["round"] == r)
                            for r in range(args.rounds)]
        ratios = [a / b for a, b in zip(medians["current"], medians["baseline"], strict=True)]
        low, high = interval(ratios, rng)
        summary[name] = {"process_medians_ns": medians, "ratio": statistics.median(ratios),
                         "ci_low": low, "ci_high": high,
                         "decision": "regression" if low > 1.10 else "pass" if high <= 1.10 else "inconclusive"}
    report = {"format": 1, "metric": "native launch to exit including workload",
              "platform": platform.platform(), "tools": tools, "rounds": args.rounds, "passes": args.passes,
              "warmup": args.warmup, "identities": identities, "summary": summary}
    (args.out / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(summary, indent=2))
    if any(row["decision"] == "regression" for row in summary.values()):
        return 1
    return 2 if any(row["decision"] == "inconclusive" for row in summary.values()) else 0

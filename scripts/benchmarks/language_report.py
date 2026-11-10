#!/usr/bin/env python3
"""Compare optimized Dream with Rust and Go on matched benchmark workloads.

Micro and macro support Rust and Go. Each process discards one internal
warmup before one measured pass. Process order rotates between rounds. Reports
retain raw samples and build identities. Checksums, row sets and iteration counts
must agree; timing is descriptive and never a merge gate.

Examples:
  scripts/bench.py compare --suite micro --rounds 3
  scripts/bench.py compare --suite micro --against rust
  scripts/bench.py compare --suite macro --against go
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import shutil
import statistics
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RUST_MANIFEST = ROOT / "tests/bench/compare/rust/Cargo.toml"
SUITES = {
    "micro": (ROOT / "tests/bench/microbenches.dream", ("rust", "go")),
    "macro": (ROOT / "tests/bench/macro/macrobenches.dream", ("rust", "go")),
}
MIN_SAMPLE_NS = 10_000


def workload_contract(name):
    if name in ("async_poll", "scheduler_queue", "async_fanout", "task_parallel"):
        return "runtime", "Native async/parallel runtimes; direct await, joining, yielding and spawning have distinct contracts."
    if name in ("json_serialize", "json_deserialize", "json_service", "regex_find", "log_processor"):
        return "library", "Native JSON/regex libraries implement the same workload, with different engines and representations."
    if name in ("char_scan", "byte_scan"):
        return "representation", "ASCII text in native encodings: UTF-16 for Dream, UTF-8 for Rust/Go; byte counts differ."
    return "workload", "Matched algorithms and logical inputs; native ownership, containers and compiler optimizations apply."


def run(command, env=None, timeout=1800):
    result = subprocess.run(command, env=env, text=True, capture_output=True, timeout=timeout)
    if result.returncode:
        sys.stderr.write(result.stdout + result.stderr)
        raise ValueError(f"command failed ({result.returncode}): {' '.join(map(str, command))}")
    return result


def parse_rows(text):
    rows = {}
    for line in text.splitlines():
        if not line.startswith("bench "):
            continue
        parts = line.split()
        fields = dict(tok.split("=", 1) for tok in parts[2:] if "=" in tok)
        try:
            name = parts[1]
            row = {key: int(fields[key]) for key in ("ns_total", "iters", "checksum")}
        except (KeyError, ValueError, IndexError) as error:
            raise ValueError(f"malformed benchmark row: {line}") from error
        if name in rows:
            raise ValueError(f"duplicate benchmark row: {name}")
        if row["ns_total"] < 0 or row["iters"] <= 0:
            raise ValueError(f"{name}: timings must be nonnegative and iteration counts must be positive")
        rows[name] = row
    if not rows:
        raise ValueError("benchmark produced no bench rows")
    return rows


def order_for(round_index, arms):
    offset = round_index % len(arms)
    return arms[offset:] + arms[:offset]


def timing_environment(seed):
    if os.environ.get("DREAM_NATIVE_SANITIZE") or any(
        os.environ.get(key) == "1" for key in ("DREAM_RUNTIME_COUNTERS", "DREAM_BENCH_COUNTERS")
    ):
        raise ValueError("language timing comparisons require Dream sanitizers and counters to be disabled")
    return dict(os.environ, DREAM_BENCH_SEED=seed, DREAM_BENCH_PASSES="1")


def summarize(samples):
    names = list(samples["dream"][0])
    expected = set(names)
    rounds = len(samples["dream"])
    for arm, processes in samples.items():
        if len(processes) != rounds:
            raise ValueError(f"{arm}: incomplete process samples")
        for process in processes:
            if set(process) != expected:
                raise ValueError(f"{arm}: workload names differ: missing={sorted(expected - set(process))}, extra={sorted(set(process) - expected)}")
    rows = []
    for name in names:
        reference = samples["dream"][0][name]
        measurements = {}
        for arm, processes in samples.items():
            for process in processes:
                for key in ("checksum", "iters"):
                    if process[name][key] != reference[key]:
                        raise ValueError(f"{name}: {arm} {key}={process[name][key]} differs from Dream {reference[key]}")
            values = [p[name]["ns_total"] / p[name]["iters"] for p in processes]
            measurements[arm] = {"median_ns_per_op": statistics.median(values), "ns_per_op": values,
                                 "ns_total": [p[name]["ns_total"] for p in processes]}
        comparisons = {}
        for arm in samples:
            if arm == "dream":
                continue
            comparator = measurements[arm]["median_ns_per_op"]
            ratio = measurements["dream"]["median_ns_per_op"] / comparator if comparator else None
            if min(*measurements["dream"]["ns_total"], *measurements[arm]["ns_total"]) < MIN_SAMPLE_NS:
                verdict = "too short"
            else:
                verdict = "Dream" if ratio < .9 else arm.title() if ratio > 1.1 else "tie"
            if arm == "go" and name in ("async_poll", "scheduler_queue"):
                verdict = "not equivalent"
            comparisons[arm] = {"ratio": ratio, "verdict": verdict}
        kind, contract = workload_contract(name)
        rows.append({"name": name, "iters": reference["iters"], "checksum": reference["checksum"],
                     "kind": kind, "contract": contract,
                     "measurements": measurements, "comparisons": comparisons})
    return rows


def fingerprint(paths):
    return {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in paths}


def compile_arm(arm, args, out):
    suffix = ".exe" if os.name == "nt" else ""
    binary = out / f"{args.suite}-{arm}{suffix}"
    if arm == "dream":
        source = SUITES[args.suite][0]
        command = [str(args.dream), "--release", "-o", str(binary), str(source)]
        version = run([str(args.dream), "--version"]).stdout.strip()
        sources = [source]
    elif arm == "go":
        source = ROOT / f"tests/bench/compare/{args.suite}.go"
        command = [args.go, "build", "-o", str(binary), str(source)]
        version = run([args.go, "version"]).stdout.strip()
        sources = [source]
    else:
        target = ROOT / "target/bench-rust"
        command = [args.cargo, "build", "--release", "--locked", "--manifest-path", str(RUST_MANIFEST), "--target-dir", str(target)]
        version = run([args.rustc, "--version"]).stdout.strip()
        sources = [ROOT / "Cargo.toml", ROOT / "Cargo.lock", RUST_MANIFEST, *sorted(RUST_MANIFEST.parent.glob("src/*.rs"))]
    print(f"compile {arm} {args.suite}", flush=True)
    build_env = dict(os.environ, CARGO_INCREMENTAL="0")
    if arm == "rust":
        build_env["RUSTC"] = args.rustc
    run(command, build_env)
    if arm == "dream":
        binary = binary.with_suffix(".bin")
    elif arm == "rust":
        shutil.copy2(target / f"release/dream-{args.suite}-rust{suffix}", binary)
    if not binary.is_file():
        raise ValueError(f"{arm} did not write {binary}")
    compiler = Path(shutil.which(args.rustc if arm == "rust" else command[0]) or command[0]).resolve()
    settings = ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_PROFILE_RELEASE_OPT_LEVEL", "CARGO_PROFILE_RELEASE_LTO",
                "GOFLAGS", "GOGC", "GOMEMLIMIT", "GOMAXPROCS", "RAYON_NUM_THREADS")
    return binary, {"command": command, "version": version, "sources": fingerprint(sources),
                    "compiler": str(compiler), "compiler_sha256": hashlib.sha256(compiler.read_bytes()).hexdigest(),
                    "environment": {key: build_env[key] for key in settings if key in build_env},
                    "binary": str(binary), "sha256": hashlib.sha256(binary.read_bytes()).hexdigest()}


def markdown(report):
    arms = report["against"]
    note = (f"Median of {report['rounds']} rounds after one warmup. Seed `{report['seed']}`. "
            "A ratio below 1 means Dream was faster; 0.90–1.10 is a tie.")
    if any(row["comparisons"].get(arm, {}).get("verdict") == "not equivalent" for row in report["rows"] for arm in arms):
        note += " Go async rows that spawn goroutines are not scored."
    header = ["Bench", "Iterations", "Dream ns/op"]
    header += [f"{arm.title()} ns/op" for arm in arms]
    header += [f"Dream/{arm.title()}" for arm in arms]
    header += [f"vs {arm.title()}" for arm in arms]
    rows = []
    for row in report["rows"]:
        cells = [row["name"], str(row["iters"]), f"{row['measurements']['dream']['median_ns_per_op']:,.2f}"]
        cells += [f"{row['measurements'][arm]['median_ns_per_op']:,.2f}" for arm in arms]
        cells += [f"{row['comparisons'][arm]['ratio']:.3f}" if row['comparisons'][arm]['ratio'] is not None else "—" for arm in arms]
        cells += [row["comparisons"][arm]["verdict"] for arm in arms]
        rows.append("| " + " | ".join(cells) + " |")
    align = "|" + "|".join(["---"] + ["---:"] * (len(header) - 1)) + "|"
    lines = [f"# Dream {report['suite']} vs {' and '.join(arm.title() for arm in arms)}", "", note, "",
             "| " + " | ".join(header) + " |", align, *rows, "", "Checksums and iteration counts match.", ""]
    for row in report["rows"]:
        if row["name"] == "byte_scan":
            ratios = [f"Dream/{arm.title()} {row['comparisons'][arm]['ratio'] / 2:.3f}"
                      for arm in arms if row['comparisons'][arm]['ratio'] is not None]
            lines += ["ASCII byte scan reads two UTF-16 bytes per character in Dream and one UTF-8 byte in Rust/Go. "
                      + "Time ratios per byte: " + ", ".join(ratios) + ".", ""]
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--dream", type=Path, default=ROOT / "target/release/dream")
    parser.add_argument("--go", default=shutil.which("go") or "go")
    parser.add_argument("--cargo", default=shutil.which("cargo") or "cargo")
    parser.add_argument("--rustc", default=shutil.which("rustc") or "rustc")
    parser.add_argument("--suite", choices=sorted(SUITES), default="micro")
    parser.add_argument("--against", action="append", choices=["rust", "go"])
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--seed", choices=["0", "1"], default="1")
    parser.add_argument("--out", type=Path, default=ROOT / "target/language-report")
    args = parser.parse_args()
    if not 1 <= args.rounds <= 10:
        parser.error("--rounds must be from 1 to 10")
    args.against = list(dict.fromkeys(args.against or SUITES[args.suite][1]))
    if unsupported := set(args.against) - set(SUITES[args.suite][1]):
        parser.error(f"{args.suite} does not have ports for: {', '.join(sorted(unsupported))}")
    args.dream = args.dream.resolve()
    if not args.dream.is_file():
        parser.error(f"Dream compiler not found: {args.dream}; run cargo build --release -p dream")
    env = timing_environment(args.seed)
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    sample_dir = out / "samples"
    sample_dir.mkdir(exist_ok=True)
    arms = ["dream", *args.against]
    builds = {arm: compile_arm(arm, args, out) for arm in arms}
    samples = {arm: [] for arm in arms}
    orders = []
    for index in range(args.rounds):
        order = order_for(index, arms)
        orders.append(order)
        for arm in order:
            print(f"run {arm} {args.suite} round {index + 1}/{args.rounds}", flush=True)
            result = run([str(builds[arm][0])], env, timeout=300)
            (sample_dir / f"{arm}-{index + 1}.txt").write_text(result.stdout)
            samples[arm].append(parse_rows(result.stdout))
    report = {"schema_version": 1, "suite": args.suite, "against": args.against, "rounds": args.rounds,
              "seed": args.seed, "platform": platform.platform(), "machine": platform.machine(),
              "captured_at": datetime.now(timezone.utc).isoformat(), "cpu_count": os.cpu_count(),
              "protocol": {"warmups_per_process": 1, "measured_passes_per_process": 1},
              "harness": fingerprint([Path(__file__).resolve(), ROOT / "scripts/bench.py"]),
              "order": orders, "builds": {arm: build[1] for arm, build in builds.items()},
              "rows": summarize(samples)}
    text = markdown(report)
    (out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    (out / "report.md").write_text(text)
    if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
        with Path(summary).open("a") as handle:
            handle.write(text)
    print(text)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ValueError, subprocess.TimeoutExpired) as error:
        raise SystemExit(str(error)) from error

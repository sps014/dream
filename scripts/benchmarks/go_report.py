#!/usr/bin/env python3
"""Report micro and macro bench time against an optimized Go build.

Dream is compiled with `--release` (LLVM -O3). Go is `go build` with the
compiler's default optimizations; `-gcflags=all=-N -l` is not passed. Each
process warms up once internally, then one measured pass is kept. The median
of those passes is the row.

A checksum mismatch means the Go program no longer does the Dream workload, so
the process exits non-zero. Timing never fails the run. This is a manual
report, not a merge gate.
"""
from __future__ import annotations

import argparse
import json
import os
import shutil
import statistics
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SUITES = {
    "micro": (
        ROOT / "tests/bench/microbenches.dream",
        ROOT / "tests/bench/compare/micro.go",
    ),
    "macro": (
        ROOT / "tests/bench/macro/macrobenches.dream",
        ROOT / "tests/bench/compare/macro.go",
    ),
}


def run(command: list[str], env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(command, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if result.returncode != 0:
        sys.stderr.write(result.stdout)
        sys.stderr.write(result.stderr)
        raise SystemExit(f"command failed ({result.returncode}): {' '.join(command)}")
    return result


def parse_rows(text: str) -> dict[str, tuple[float, int]]:
    rows: dict[str, tuple[float, int]] = {}
    for line in text.splitlines():
        if not line.startswith("bench "):
            continue
        parts = line.split()
        fields = dict(tok.split("=", 1) for tok in parts[2:] if "=" in tok)
        iters = int(fields["iters"])
        rows[parts[1]] = (int(fields["ns_total"]) / iters, int(fields["checksum"]))
    if not rows:
        raise SystemExit("benchmark produced no bench rows")
    return rows


def median_runs(binary: Path, rounds: int, env: dict[str, str]) -> dict[str, tuple[float, int]]:
    parsed = []
    for _ in range(rounds):
        result = run([str(binary)], env)
        parsed.append(parse_rows(result.stdout))
    names = list(parsed[0])
    out: dict[str, tuple[float, int]] = {}
    for name in names:
        samples = [rows[name][0] for rows in parsed]
        checks = {rows[name][1] for rows in parsed}
        if len(checks) != 1:
            raise SystemExit(f"{binary.name} {name}: checksum changed across runs {sorted(checks)}")
        out[name] = (statistics.median(samples), checks.pop())
    return out


def fmt_ns(value: float) -> str:
    return f"{value:,.2f}" if value < 100 else f"{value:,.0f}"


def verdict(ratio: float) -> str:
    if ratio < 0.9:
        return "Dream"
    if ratio > 1.1:
        return "Go"
    return "tie"


def markdown(report: dict) -> str:
    lines = [
        "# Dream micro and macro vs Go",
        "",
        f"Dream `{report['dream']}` compiled with `--release`. Go `{report['go']}` "
        "compiled with default optimizations (`go build`, not `-N -l`).",
        f"Median of {report['rounds']} measured passes after one warmup inside each process. "
        f"`DREAM_BENCH_SEED={report['seed']}`. `dream/go` below 1 means Dream used less time per operation.",
        "A ratio inside 0.90–1.10 is a tie. Async and task rows are Dream's scheduler against Go goroutines.",
        "",
    ]
    for suite, body in report["suites"].items():
        lines.append(f"## {suite}")
        lines.append("")
        lines.append("| Bench | Dream ns/op | Go ns/op | Dream/Go | |")
        lines.append("|---|---:|---:|---:|---|")
        for row in body["rows"]:
            lines.append(
                f"| {row['name']} | {fmt_ns(row['dream_ns_per_op'])} | {fmt_ns(row['go_ns_per_op'])} | "
                f"{row['ratio']:.2f} | {row['verdict']} |"
            )
        lines.append("")
        lines.append(
            f"Dream faster: {body['dream_faster']}. Tie: {body['tie']}. Go faster: {body['go_faster']}."
        )
        lines.append("")
    return "\n".join(lines)


def compile_suite(name: str, dream: Path, go: str, out: Path) -> tuple[Path, Path]:
    source, go_source = SUITES[name]
    suite_out = out / name
    suite_out.mkdir(parents=True, exist_ok=True)
    dream_bin = suite_out / name
    go_bin = suite_out / f"{name}-go"
    print(f"compile dream {name}", flush=True)
    run([str(dream), "--release", "-o", str(dream_bin), str(source)])
    produced = dream_bin.with_suffix(".bin")
    if not produced.is_file():
        raise SystemExit(f"dream did not write {produced}")
    print(f"compile go {name}", flush=True)
    run([go, "build", "-o", str(go_bin), str(go_source)])
    return produced, go_bin


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dream", type=Path, default=ROOT / "target" / "release" / "dream")
    parser.add_argument("--go", default=shutil.which("go") or "go")
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--seed", default="1")
    parser.add_argument("--suite", action="append", choices=sorted(SUITES), dest="suites")
    parser.add_argument("--out", type=Path, default=ROOT / "target" / "go-report")
    args = parser.parse_args()
    if not 1 <= args.rounds <= 10:
        parser.error("--rounds must be from 1 to 10")
    dream = args.dream.resolve()
    if not dream.is_file():
        raise SystemExit(f"dream binary not found at {dream}; cargo build --release -p dream")
    suites = args.suites or ["micro", "macro"]
    go_version = run([args.go, "version"]).stdout.strip()
    env = dict(os.environ, DREAM_BENCH_SEED=args.seed, DREAM_BENCH_PASSES="1")
    report: dict = {
        "dream": str(dream),
        "go": go_version,
        "rounds": args.rounds,
        "seed": args.seed,
        "suites": {},
    }
    mismatched = False
    for name in suites:
        dream_bin, go_bin = compile_suite(name, dream, args.go, args.out)
        print(f"run dream {name}", flush=True)
        dream_rows = median_runs(dream_bin, args.rounds, env)
        print(f"run go {name}", flush=True)
        go_rows = median_runs(go_bin, args.rounds, env)
        rows = []
        counts = {"dream_faster": 0, "tie": 0, "go_faster": 0}
        for bench, (dream_ns, dream_check) in dream_rows.items():
            if bench not in go_rows:
                raise SystemExit(f"{name}: Go is missing bench {bench}")
            go_ns, go_check = go_rows[bench]
            if dream_check != go_check:
                mismatched = True
                sys.stderr.write(f"{name} {bench}: checksum dream={dream_check} go={go_check}\n")
            ratio = dream_ns / go_ns if go_ns else float("inf")
            label = verdict(ratio)
            counts[{"Dream": "dream_faster", "Go": "go_faster", "tie": "tie"}[label]] += 1
            rows.append({
                "name": bench,
                "dream_ns_per_op": dream_ns,
                "go_ns_per_op": go_ns,
                "ratio": ratio,
                "checksum": dream_check,
                "go_checksum": go_check,
                "verdict": label,
            })
        missing = sorted(set(go_rows) - set(dream_rows))
        if missing:
            raise SystemExit(f"{name}: Dream is missing benches {', '.join(missing)}")
        report["suites"][name] = {**counts, "rows": rows}
    args.out.mkdir(parents=True, exist_ok=True)
    text = markdown(report)
    (args.out / "report.md").write_text(text)
    (args.out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with Path(summary).open("a") as handle:
            handle.write(text)
    sys.stdout.write(text)
    if mismatched:
        raise SystemExit("checksum mismatch; the Go port and the Dream bench disagree")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

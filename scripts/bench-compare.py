#!/usr/bin/env python3
"""Controlled Dream-vs-Dream-vs-C# microbenchmark comparison.

Arms:
  current   target/release/dream compiling tests/bench/microbenches.dream
  baseline  a reference Dream build (default: target/bench-baseline worktree)
  csharp    tests/bench/csharp Release build, run through its compiled assembly

Every arm compiles once. Rounds rotate and alternate the arm order (ABBA-style) so drift in
CPU frequency or thermal state is spread over all arms. Raw per-pass samples are kept as JSONL;
the summary reports the median with a bootstrap 95% confidence interval, peak RSS per process,
and runtime counters when the binary was built with them.

Examples:
  scripts/bench-compare.py --rounds 6 --passes 5
  scripts/bench-compare.py --arms current --filter binary_trees weak_tree
  scripts/bench-compare.py --arms current --save-baseline tests/bench/results/dream-baseline.json
  scripts/bench-compare.py --arms current --gate tests/bench/results/dream-baseline.json
"""
from __future__ import annotations

import argparse
import json
import os
import platform
import random
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BENCH = ROOT / "tests/bench/microbenches.dream"
CSHARP = ROOT / "tests/bench/csharp"
# The reference compiler predates collector-managed recursive classes and requires the opt-in.
BASELINE_CYCLE_CLASSES = ("TreeNode", "LinkNode", "WeakTreeNode")
# Rows below this are timer noise or a deleted loop and cannot substantiate a claim.
MIN_NS_PER_OP = 0.25


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--arms", nargs="+", default=["baseline", "current", "csharp"],
                   choices=["baseline", "current", "csharp"])
    p.add_argument("--current-dream", default=str(ROOT / "target/release/dream"))
    p.add_argument("--baseline-dream", default=str(ROOT / "target/bench-baseline/target/release/dream"))
    p.add_argument("--rounds", type=int, default=6, help="measured process starts per arm")
    p.add_argument("--warmup", type=int, default=1, help="discarded process starts per arm")
    p.add_argument("--passes", type=int, default=5, help="DREAM_BENCH_PASSES per process")
    p.add_argument("--filter", nargs="*", default=[], help="only report these bench names")
    p.add_argument("--out", default=str(ROOT / "tests/bench/out/compare"))
    p.add_argument("--counters", action="store_true", help="set DREAM_BENCH_COUNTERS=1 for Dream arms")
    p.add_argument("--seed", type=int, default=0x5eed)
    p.add_argument("--save-baseline", help="write current-arm medians to this JSON file")
    p.add_argument("--gate", help="fail if current regresses >10%% beyond its CI vs this JSON")
    p.add_argument("--gate-threshold", type=float, default=0.10)
    p.add_argument("--allow-tiny", action="store_true", help="do not fail on zero-time rows")
    return p.parse_args()


def run(cmd, cwd=None, env=None):
    proc = subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, text=True)
    if proc.returncode != 0:
        sys.stderr.write(proc.stdout[-4000:] + proc.stderr[-4000:])
        raise SystemExit(f"command failed: {' '.join(map(str, cmd))}")
    return proc.stdout


def tool_versions(args) -> dict:
    out = {"host": platform.platform(), "python": platform.python_version()}
    for arm, exe in (("current", args.current_dream), ("baseline", args.baseline_dream)):
        if arm in args.arms:
            out[f"{arm}_dream"] = exe
            try:
                out[f"{arm}_version"] = run([exe, "--version"]).strip()
            except SystemExit:
                out[f"{arm}_version"] = "unknown"
    tools = ROOT / "src/execution/llvm/tools.rs"
    m = re.search(r'LLVM_VERSION[^"]*"([^"]+)"', tools.read_text()) if tools.exists() else None
    out["llvm"] = m.group(1) if m else "unknown"
    if "csharp" in args.arms and shutil.which("dotnet"):
        out["dotnet"] = run(["dotnet", "--version"]).strip()
    return out


def baseline_source(src: str) -> str:
    """The same benchmark bodies, minus instrumentation the reference compiler cannot build.

    `// bench-compare: current-only begin`/`end` blocks are dropped, and a line ending in
    `// bench-compare: baseline=<text>` is replaced by `<text>` at the same indentation.
    """
    src = re.sub(r"^// bench-compare: current-only begin\n.*?^// bench-compare: current-only end\n",
                 "", src, flags=re.M | re.S)
    src = re.sub(r"^(\s*).*// bench-compare: baseline=(.*)$", r"\1\2", src, flags=re.M)
    for name in BASELINE_CYCLE_CLASSES:
        src = re.sub(rf"^class {name}\b", f"@allow_cycle\nclass {name}", src, flags=re.M)
    return src


def prepare_dream(arm: str, exe: str, out: Path) -> list[str]:
    if not Path(exe).is_file():
        raise SystemExit(f"{arm}: dream binary not found at {exe}")
    work = out / arm
    shutil.rmtree(work, ignore_errors=True)
    work.mkdir(parents=True)
    src = BENCH.read_text()
    if arm == "baseline":
        src = baseline_source(src)
    path = work / "microbenches.dream"
    path.write_text(src)
    run([exe, "--release", str(path)], cwd=work)
    binary = work / "target/release/microbenches.bin"
    if not binary.is_file():
        raise SystemExit(f"{arm}: compile produced no {binary}")
    return [str(binary)]


def prepare_csharp() -> list[str]:
    if not shutil.which("dotnet"):
        raise SystemExit("csharp: dotnet not found")
    run(["dotnet", "build", "-c", "Release", "--nologo", "-v", "q"], cwd=CSHARP)
    dlls = sorted(CSHARP.glob("bin/Release/*/DreamBench.dll"))
    if not dlls:
        raise SystemExit("csharp: DreamBench.dll not found after build")
    return ["dotnet", str(dlls[-1])]


def run_once(cmd: list[str], env: dict) -> tuple[str, int]:
    """Runs one process and returns stdout plus its own peak RSS in bytes."""
    # Output goes to files and the child is reaped here, so wait4 reports this process alone.
    with tempfile.TemporaryFile("w+") as out, tempfile.TemporaryFile("w+") as err:
        proc = subprocess.Popen(cmd, env=env, stdout=out, stderr=err, text=True)
        _, status, usage = os.wait4(proc.pid, 0)
        proc.returncode = os.waitstatus_to_exitcode(status)
        out.seek(0)
        err.seek(0)
        stdout, stderr = out.read(), err.read()
    if proc.returncode != 0 or re.search(r"^panic:", stdout + stderr, re.M):
        sys.stderr.write(stdout[-3000:] + stderr[-3000:])
        raise SystemExit(f"run failed: {' '.join(cmd)}")
    rss = usage.ru_maxrss if sys.platform == "darwin" else usage.ru_maxrss * 1024
    return stdout, rss


def parse_lines(text: str):
    for line in text.splitlines():
        if not line.startswith("bench "):
            continue
        parts = line.split()
        kv = dict(tok.split("=", 1) for tok in parts[2:] if "=" in tok)
        row = {"name": parts[1]}
        if "ns_total" in kv and "iters" in kv:
            row["ns_total"] = float(kv["ns_total"])
            row["iters"] = int(kv["iters"])
            row["ns_per_op"] = row["ns_total"] / row["iters"] if row["iters"] else 0.0
        else:
            row["ns_per_op"] = float(kv.get("ns_per_op", "nan"))
        if "counters" in kv:
            row["counters"] = {k: int(v) for k, v in (c.split(":", 1) for c in kv["counters"].split(",") if ":" in c)}
        yield row


def bootstrap_ci(values: list[float], rng: random.Random, n: int = 2000) -> tuple[float, float]:
    if len(values) < 2:
        return (values[0], values[0]) if values else (float("nan"), float("nan"))
    meds = sorted(statistics.median(rng.choices(values, k=len(values))) for _ in range(n))
    return meds[int(0.025 * n)], meds[int(0.975 * n) - 1]


def order_for(round_index: int, arms: list[str]) -> list[str]:
    rotated = arms[round_index % len(arms):] + arms[:round_index % len(arms)]
    return rotated if round_index % 2 == 0 else list(reversed(rotated))


def summarize(samples: list[dict], rss: dict, arms: list[str], rng: random.Random, names_filter):
    by = {}
    counters = {}
    for s in samples:
        by.setdefault((s["name"], s["arm"]), []).append(s["ns_per_op"])
        if "counters" in s:
            counters.setdefault((s["name"], s["arm"]), s["counters"])
    names = sorted({n for n, _ in by})
    if names_filter:
        names = [n for n in names if n in names_filter]
    summary = {}
    for name in names:
        summary[name] = {}
        for arm in arms:
            vals = by.get((name, arm))
            if not vals:
                continue
            lo, hi = bootstrap_ci(vals, rng)
            summary[name][arm] = {
                "median": statistics.median(vals), "ci_low": lo, "ci_high": hi, "samples": len(vals),
                **({"counters": counters[(name, arm)]} if (name, arm) in counters else {}),
            }
    return summary, {arm: max(v) if v else 0 for arm, v in rss.items()}


def fmt(v: float) -> str:
    return f"{v:,.2f}" if v < 100 else f"{v:,.0f}"


def print_table(summary: dict, peak: dict, arms: list[str]) -> None:
    head = f"{'bench':<22}" + "".join(f"{arm + ' ns/op [95% CI]':>34}" for arm in arms)
    if "current" in arms and "baseline" in arms:
        head += f"{'cur/base':>10}"
    if "current" in arms and "csharp" in arms:
        head += f"{'cur/C#':>9}"
    print(head)
    print("-" * len(head))
    for name, row in summary.items():
        line = f"{name:<22}"
        for arm in arms:
            r = row.get(arm)
            cell = f"{fmt(r['median'])} [{fmt(r['ci_low'])}, {fmt(r['ci_high'])}]" if r else "-"
            line += f"{cell:>34}"
        for other in ("baseline", "csharp"):
            if "current" in arms and other in arms:
                c, o = row.get("current"), row.get(other)
                ratio = f"{c['median'] / o['median']:.2f}x" if c and o and o["median"] > 0 else "-"
                line += f"{ratio:>10}" if other == "baseline" else f"{ratio:>9}"
        print(line)
    print()
    print("peak RSS (max over processes): " + ", ".join(f"{a}={peak.get(a, 0) / 1e6:.1f} MB" for a in arms))
    with_counters = [(n, a, r[a]["counters"]) for n, r in summary.items() for a in arms if a in r and "counters" in r[a]]
    if with_counters:
        print("\nruntime counters (one measured pass):")
        for name, arm, c in with_counters:
            print(f"  {name:<22} {arm:<9} " + " ".join(f"{k}={v}" for k, v in c.items() if v))


def gate(summary: dict, path: str, threshold: float) -> int:
    stored = json.loads(Path(path).read_text())["medians"]
    failures = []
    for name, base in stored.items():
        r = summary.get(name, {}).get("current")
        if not r or base <= 0:
            continue
        # Fail only when the whole confidence interval sits beyond the allowed regression.
        if r["ci_low"] > base * (1 + threshold):
            failures.append(f"{name}: {fmt(r['median'])} ns/op (CI low {fmt(r['ci_low'])}) vs baseline {fmt(base)}")
    for f in failures:
        print("REGRESSION " + f)
    return 1 if failures else 0


def main() -> int:
    args = parse_args()
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    rng = random.Random(args.seed)
    commands = {}
    for arm in args.arms:
        if arm == "csharp":
            commands[arm] = prepare_csharp()
        else:
            exe = args.current_dream if arm == "current" else args.baseline_dream
            commands[arm] = prepare_dream(arm, exe, out)
    env = dict(os.environ, DREAM_BENCH_PASSES=str(args.passes))
    dream_env = dict(env, **({"DREAM_BENCH_COUNTERS": "1"} if args.counters else {}))
    samples: list[dict] = []
    rss = {arm: [] for arm in args.arms}
    raw_path = out / "raw.jsonl"
    with raw_path.open("w") as raw:
        for r in range(args.warmup + args.rounds):
            for arm in order_for(r, args.arms):
                started = time.time()
                stdout, peak = run_once(commands[arm], env if arm == "csharp" else dream_env)
                if r < args.warmup:
                    continue
                rss[arm].append(peak)
                seen: dict[str, int] = {}
                for row in parse_lines(stdout):
                    seen[row["name"]] = seen.get(row["name"], -1) + 1
                    row.update(arm=arm, round=r - args.warmup, pass_index=seen[row["name"]],
                               peak_rss=peak, started=started)
                    raw.write(json.dumps(row) + "\n")
                    samples.append(row)
            print(f"round {r + 1}/{args.warmup + args.rounds} done", file=sys.stderr)
    summary, peak = summarize(samples, rss, args.arms, rng, set(args.filter))
    (out / "summary.json").write_text(json.dumps(
        {"tools": tool_versions(args), "passes": args.passes, "rounds": args.rounds,
         "peak_rss": peak, "benchmarks": summary}, indent=2))
    print_table(summary, peak, args.arms)
    print(f"\nraw samples: {raw_path}\nsummary: {out / 'summary.json'}")
    status = 0
    if not args.allow_tiny:
        tiny = sorted({s["name"] for s in samples if s["arm"] != "csharp"
                       and (s.get("ns_total") == 0 or s["ns_per_op"] < MIN_NS_PER_OP)})
        if tiny:
            print("ZERO-TIME rows (measured work was optimized away): " + ", ".join(tiny))
            status = 1
    if args.save_baseline and "current" in args.arms:
        medians = {n: r["current"]["median"] for n, r in summary.items() if "current" in r}
        Path(args.save_baseline).write_text(json.dumps({"tools": tool_versions(args), "medians": medians}, indent=2) + "\n")
    if args.gate:
        status |= gate(summary, args.gate, args.gate_threshold)
    return status


if __name__ == "__main__":
    sys.exit(main())

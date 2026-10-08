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
import bench_gate

ROOT = Path(__file__).resolve().parent.parent
BENCH = ROOT / "tests/bench/microbenches.dream"
CSHARP = ROOT / "tests/bench/csharp"
# Rows below this are timer noise or a deleted loop and cannot substantiate a claim.
MIN_NS_PER_OP = 0.25


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--arms", nargs="+", default=["baseline", "current", "csharp"],
                   choices=["baseline", "current", "csharp"])
    p.add_argument("--current-dream", default=str(ROOT / "target/release/dream"))
    p.add_argument("--baseline-dream", default=str(ROOT / "target/bench-baseline/target/release/dream"))
    p.add_argument("--rounds", type=int, default=10, help="measured process starts per arm (10-20 for gating)")
    p.add_argument("--warmup", type=int, default=1, help="discarded process starts per arm")
    p.add_argument("--passes", type=int, default=5, help="DREAM_BENCH_PASSES per process")
    p.add_argument("--filter", nargs="*", default=[], help="only report these bench names")
    p.add_argument("--out", default=str(ROOT / "tests/bench/out/compare"))
    p.add_argument("--counters", action="store_true", help="set DREAM_BENCH_COUNTERS=1 for Dream arms")
    p.add_argument("--seed", type=int, default=0x5eed)
    p.add_argument("--save-baseline", help="save a validated immutable reference and complete samples")
    p.add_argument("--gate", help="compare current against the saved reference in paired rounds")
    p.add_argument("--runner-id", default=os.environ.get("DREAM_BENCH_RUNNER_ID"), help="controlled runner identity (required for gating)")
    p.add_argument("--input-seed", type=int, default=1)
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
        if arm in args.arms and not (arm == 'baseline' and args.gate):
            out[f"{arm}_dream"] = exe
            out[f"{arm}_sha256"] = bench_gate.digest(exe)
            try:
                out[f"{arm}_version"] = run([exe, "--version"]).strip()
            except SystemExit:
                out[f"{arm}_version"] = "unknown"
    tools = ROOT / "src/execution/llvm/tools.rs"
    m = re.search(r'LLVM_VERSION[^"]*"([^"]+)"', tools.read_text()) if tools.exists() else None
    out["llvm"] = m.group(1) if m else "unknown"
    llvm = Path(os.environ.get("DREAM_LLVM", str(Path.home() / ".dream/toolchains" / f"llvm-{out['llvm']}" / "bin")))
    if (llvm / "bin").is_dir():
        llvm /= "bin"
    for name in ("clang", "opt", "llc", "llvm-link"):
        executable = llvm / name
        if executable.is_file():
            out[f"{name}_sha256"] = bench_gate.digest(executable)
            out[f"{name}_version"] = run([str(executable), "--version"]).strip()
        else:
            out[f"{name}_version"] = "unknown"
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
    return src


def prepare_dream(arm: str, exe: str, out: Path, counters: bool = False) -> list[str]:
    exe = str(Path(exe).resolve())
    out = out.resolve()
    if not Path(exe).is_file():
        raise SystemExit(f"{arm}: dream binary not found at {exe}")
    work = out / arm
    work.mkdir(parents=True, exist_ok=True)
    src = BENCH.read_text()
    if arm == "baseline":
        src = baseline_source(src)
    path = work / "microbenches.dream"
    if not path.exists() or path.read_text() != src:
        path.write_text(src)
    compile_env = dict(os.environ, DREAM_RUNTIME_COUNTERS="1" if counters and arm == "current" else "0")
    run([exe, "--release", str(path)], cwd=work, env=compile_env)
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
    return rotated if (round_index // len(arms)) % 2 == 0 else list(reversed(rotated))


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


def main() -> int:
    args = parse_args()
    if args.gate and args.save_baseline:
        raise ValueError("choose either reference creation or gating")
    if args.warmup < 1 or args.rounds < 1 or args.passes < 1 or not 0 <= args.input_seed <= 2147483647:
        raise ValueError("invalid warmup, round, pass count or input seed")
    reference = None
    compatibility = bench_gate.identity(BENCH, CSHARP / "Program.cs", args.runner_id, args.input_seed, args.passes)
    if args.gate or args.save_baseline:
        if os.environ.get("DREAM_NATIVE_SANITIZE"):
            raise ValueError("sanitizer instrumentation cannot form a timing reference or gate")
        if args.counters or args.allow_tiny or args.filter:
            raise ValueError("references and gates require complete, uninstrumented, valid measurements")
        if not args.runner_id or not 10 <= args.rounds <= 20 or args.passes < 5 or "current" not in args.arms:
            raise ValueError("references and gates require a runner id, 10-20 rounds, five passes and the current arm")
        if args.gate:
            reference, control = bench_gate.load(args.gate, compatibility)
            if "baseline" not in args.arms: args.arms.insert(0, "baseline")
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    rng = random.Random(args.seed)
    commands = {}
    identities = tool_versions(args)
    if reference:
        identities.update({key: value for key, value in reference['tools'].items() if key.startswith('baseline_')})
        for key in ('llvm', *(f'{name}_{suffix}' for name in ('clang', 'opt', 'llc', 'llvm-link') for suffix in ('sha256', 'version'))):
            if identities.get(key) != reference['tools'].get(key):
                raise ValueError(f'incompatible reference tool: {key}')
    for arm in args.arms:
        if arm == "baseline" and reference:
            commands[arm] = control
        elif arm == "csharp":
            commands[arm] = prepare_csharp()
        else:
            exe = args.current_dream if arm == "current" else args.baseline_dream
            commands[arm] = prepare_dream(arm, exe, out, args.counters)
    for arm, command in commands.items():
        artifact = Path(command[-1]) if arm == 'csharp' else Path(command[0])
        identities[f'{arm}_artifact_sha256'] = bench_gate.digest(artifact)
    env = dict(os.environ, DREAM_BENCH_PASSES=str(args.passes), DREAM_BENCH_SEED=str(args.input_seed))
    dream_env = dict(env, DREAM_BENCH_COUNTERS="1" if args.counters else "0")
    samples: list[dict] = []
    rss = {arm: [] for arm in args.arms}
    raw_path = out / "raw.jsonl"
    with raw_path.open("w") as raw:
        for r in range(args.warmup + args.rounds):
            for arm in order_for(r, args.arms):
                started = time.time()
                stdout, peak = run_once(commands[arm], env if arm == "csharp" else dream_env)
                sinks = re.findall(r"^sink (-?\d+)$", stdout, re.M)
                if len(sinks) != 1:
                    raise ValueError(f"{arm}: missing or duplicate result sink")
                if r < args.warmup:
                    continue
                rss[arm].append(peak)
                seen: dict[str, int] = {}
                for row in parse_lines(stdout):
                    seen[row["name"]] = seen.get(row["name"], -1) + 1
                    row.update(arm=arm, round=r - args.warmup, pass_index=seen[row["name"]],
                               peak_rss=peak, started=started, sink=int(sinks[0]))
                    raw.write(json.dumps(row) + "\n")
                    samples.append(row)
            print(f"round {r + 1}/{args.warmup + args.rounds} done", file=sys.stderr)
    summary, peak = summarize(samples, rss, args.arms, rng, set(args.filter))
    (out / "summary.json").write_text(json.dumps(
        {"tools": identities, "passes": args.passes, "rounds": args.rounds,
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
    if args.save_baseline:
        if status:
            raise ValueError("invalid timing rows cannot form a reference")
        arm = "baseline" if "baseline" in args.arms else "current"
        bench_gate.save(args.save_baseline, ROOT, compatibility, commands[arm], samples, arm,
                        args.passes, args.rounds, rss, identities)
    if args.gate:
        if status:
            raise ValueError("invalid timing rows cannot pass a gate")
        status, decisions = bench_gate.evaluate(samples, reference, args.passes, args.rounds, rss, 0.10)
        (out / "gate.json").write_text(json.dumps(decisions, indent=2) + "\n")
        for name, decision in decisions.items():
            print(f"GATE {name}: {decision['decision']} ({decision['ratio']:.3f}x)")
    return status


if __name__ == "__main__":
    try:
        sys.exit(main())
    except ValueError as error:
        sys.stderr.write(f"invalid benchmark measurement: {error}\n")
        sys.exit(3)

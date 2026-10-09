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
  scripts/bench.py                        # native Dream/C#, 20 rounds, 10 passes
  scripts/bench.py build --help           # compiler cold/warm/edited builds
  scripts/bench.py generators --help      # isolated source-generator caches
  scripts/bench.py --arms current --filter binary_trees weak_tree
  scripts/bench.py --arms current --save-baseline tests/bench/results/dream-baseline.json
  scripts/bench.py --arms current --gate tests/bench/results/dream-baseline.json
"""
from __future__ import annotations

import argparse
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from benchmarks import gate as bench_gate, stats as bench_stats
from benchmarks.process import dream_name, peak_working_set, with_exe

ROOT = Path(__file__).resolve().parent.parent
BENCH = ROOT / "tests/bench/microbenches.dream"
CSHARP = ROOT / "tests/bench/csharp"
# Rows below this are timer noise or a deleted loop and cannot substantiate a claim.
MIN_NS_PER_OP = 0.25


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--target", choices=["native", "wasm"], default="native")
    p.add_argument("--benchmark", type=Path, default=BENCH, help="fixture path, including a preserved campaign snapshot")
    p.add_argument("--node", default=shutil.which("node"), help="Node executable for wasm measurements")
    p.add_argument("--diagnostics", action="store_true", help="save optimized IR and first-pass LLVM remarks")
    p.add_argument("--arms", nargs="+", default=["current", "csharp"],
                   choices=["baseline", "current", "csharp"])
    p.add_argument("--current-dream", default=str(ROOT / "target" / "release" / dream_name()))
    p.add_argument("--baseline-dream", default=str(ROOT / "target" / "bench-baseline" / "target" / "release" / dream_name()))
    p.add_argument("--rounds", type=int, default=20, help="measured process starts per arm (10-20 for gating)")
    p.add_argument("--warmup", type=int, default=2, help="discarded process starts per arm")
    p.add_argument("--passes", type=int, default=10, help="DREAM_BENCH_PASSES per process")
    p.add_argument("--filter", nargs="*", default=[], help="only report these bench names")
    p.add_argument("--out", default=str(ROOT / "tests/bench/out/compare"))
    p.add_argument("--counters", action="store_true", help="set DREAM_BENCH_COUNTERS=1 for Dream arms")
    p.add_argument("--regression-threshold", type=float, default=0.05)
    p.add_argument("--save-baseline", help="save a validated immutable reference and complete samples")
    p.add_argument("--gate", help="compare current against the saved reference in paired rounds")
    p.add_argument("--runner-id", default=os.environ.get("DREAM_BENCH_RUNNER_ID"), help="controlled runner identity (required for gating)")
    p.add_argument("--input-seed", type=int, default=1)
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
            exe = str(with_exe(Path(exe)))
            out[f"{arm}_dream"] = exe
            out[f"{arm}_sha256"] = bench_gate.digest(exe)
            try:
                out[f"{arm}_version"] = run([exe, "--version"]).strip()
            except SystemExit:
                out[f"{arm}_version"] = "unknown"
    if args.target == "wasm":
        if not args.node or not Path(args.node).is_file():
            raise ValueError("wasm measurements require a Node executable")
        out["node_sha256"] = bench_gate.digest(args.node)
        out["node_version"] = run([args.node, "--version"]).strip()
        registry = (ROOT / "crates/dream-abi/src/toolchain.rs").read_text()
        version = re.search(r'BINARYEN_VERSION[^"\n]*"([^"]+)"', registry).group(1)
        optimizer = with_exe(Path(os.environ.get(
            "DREAM_WASM_OPT", str(Path.home() / ".dream" / "toolchains" / f"binaryen-{version}" / "bin" / "wasm-opt"))))
        out["wasm_opt_sha256"] = bench_gate.digest(optimizer)
        out["wasm_opt_version"] = run([str(optimizer), "--version"]).strip()
    tools = ROOT / "src/execution/llvm/tools.rs"
    m = re.search(r'LLVM_VERSION[^"]*"([^"]+)"', tools.read_text()) if tools.exists() else None
    out["llvm"] = m.group(1) if m else "unknown"
    llvm = Path(os.environ.get("DREAM_LLVM", str(Path.home() / ".dream/toolchains" / f"llvm-{out['llvm']}" / "bin")))
    if (llvm / "bin").is_dir():
        llvm /= "bin"
    for name in ("clang", "opt", "llc", "llvm-link"):
        executable = with_exe(llvm / name)
        if executable.is_file():
            out[f"{name}_sha256"] = bench_gate.digest(executable)
            out[f"{name}_version"] = run([str(executable), "--version"]).strip()
        else:
            out[f"{name}_version"] = "unknown"
    if "csharp" in args.arms and shutil.which("dotnet"):
        out["dotnet"] = run(["dotnet", "--version"]).strip()
        out["dotnet_info"] = run(["dotnet", "--info"]).strip()
        out["dotnet_sha256"] = bench_gate.digest(Path(shutil.which("dotnet")).resolve())
        runtimes = run(["dotnet", "--list-runtimes"])
        out["dotnet_runtimes"] = runtimes.strip()
        out["dotnet_runtime_files"] = {
            str(p): bench_gate.digest(p)
            for version, directory in re.findall(r"^Microsoft.NETCore.App (\S+) \[(.+)\]$", runtimes, re.M)
            for p in sorted((Path(directory) / version).iterdir()) if p.is_file()
        }
    out["runtime_sources"] = {str(p.relative_to(ROOT)): bench_gate.digest(p)
        for p in sorted((ROOT / "crates/dream-mir/src/runtime").rglob("*")) if p.is_file()}
    out["host_libraries"] = {str(p): bench_gate.digest(p)
        for directory in (ROOT / "target/release", Path.home() / ".dream/lib")
        if directory.is_dir() for p in sorted(directory.glob("*dream_host*")) if p.is_file()}
    return out


def prepare_dream(arm: str, exe: str, out: Path, args) -> list[str]:
    exe = str(with_exe(Path(exe)).resolve())
    out = out.resolve()
    if not Path(exe).is_file():
        raise SystemExit(f"{arm}: dream binary not found at {exe}")
    work = out / arm
    work.mkdir(parents=True, exist_ok=True)
    path = work / "microbenches.dream"
    if not path.exists() or path.read_bytes() != args.benchmark_source:
        path.write_bytes(args.benchmark_source)
    compile_env = dict(os.environ, DREAM_RUNTIME_COUNTERS="1" if args.counters else "0")
    flags = ["--release"]
    if args.diagnostics:
        flags += ["--opt-remarks", "--emit-opt-ir"]
    if args.target == "wasm":
        flags += ["--wasm", "--node"]
    started = time.monotonic()
    run([exe, *flags, str(path)], cwd=work, env=compile_env)
    artifacts = work / ("target/web" if args.target == "wasm" else "target/release")
    if args.target == "wasm":
        runtime = artifacts / "microbenches.node.runtime.js"
        shutil.copy2(runtime, runtime.with_suffix(".mjs"))
        runner = artifacts / "microbenches.mjs"
        runner.write_text('import { run } from "./microbenches.node.runtime.mjs";\n'
                          'import { fileURLToPath } from "node:url";\n'
                          'await run(fileURLToPath(new URL("./microbenches.wasm", import.meta.url)), '
                          '{ stdout: s => process.stdout.write(s) });\n')
        command = [str(Path(args.node).resolve()), str(runner)]
        artifact = artifacts / "microbenches.wasm"
    else:
        command = [str(artifacts / "microbenches.bin")]
        artifact = Path(command[0])
    if not artifact.is_file():
        raise ValueError(f"{arm}: missing compiled artifact {artifact}")
    elapsed = time.monotonic() - started
    if args.diagnostics and args.target == "native":
        version = re.search(r'LLVM_VERSION[^"\n]*"([^"]+)"', (ROOT / "src/execution/llvm/tools.rs").read_text()).group(1)
        llvm = Path(os.environ.get("DREAM_LLVM", str(Path.home() / ".dream/toolchains" / f"llvm-{version}" / "bin")))
        if (llvm / "bin").is_dir():
            llvm /= "bin"
        run([str(with_exe(llvm / "llc")), "-O3", "-mcpu=native", "-filetype=asm",
             str(artifact.with_suffix(".opt.ll")), "-o", str(artifact.with_suffix(".s"))])
    (work / "build.json").write_text(json.dumps({"compile_seconds": elapsed,
        "artifact_bytes": artifact.stat().st_size, "sha256": bench_gate.digest(artifact)}, indent=2) + "\n")
    return command


def prepare_csharp(out, args) -> list[str]:
    if not shutil.which("dotnet"):
        raise SystemExit("csharp: dotnet not found")
    work = out.resolve() / "csharp"
    work.mkdir(parents=True, exist_ok=True)
    (work / "Program.cs").write_bytes(args.csharp_source)
    (work / "DreamBench.csproj").write_bytes(args.csharp_project)
    started = time.monotonic()
    run(["dotnet", "build", "-c", "Release", "--nologo", "-v", "q"], cwd=work)
    dlls = sorted(work.glob("bin/Release/*/DreamBench.dll"))
    if not dlls:
        raise SystemExit("csharp: DreamBench.dll not found after build")
    artifact = dlls[-1]
    (work / "build.json").write_text(json.dumps({"compile_seconds": time.monotonic() - started,
        "artifact_bytes": artifact.stat().st_size, "sha256": bench_gate.digest(artifact)}, indent=2) + "\n")
    if args.diagnostics:
        env = dict(os.environ, DREAM_BENCH_PASSES="1", DREAM_BENCH_SEED=str(args.input_seed),
                   DOTNET_JitDisasm="BenchNbody Fib QsortRange",
                   DOTNET_JitStdOutFile=str(work / "jit-disassembly.txt"))
        stdout = run(["dotnet", str(artifact)], env=env)
        (work / "diagnostic-stdout.txt").write_text(stdout)
    return ["dotnet", str(artifact)]


def run_once(cmd: list[str], env: dict) -> tuple[str, int]:
    """Runs one process and returns stdout plus its own peak RSS in bytes."""
    # Output goes to files so the measured process is reaped here, not through a pipe.
    with tempfile.TemporaryFile("w+", encoding="utf-8") as out, tempfile.TemporaryFile("w+", encoding="utf-8") as err:
        proc = subprocess.Popen(cmd, env=env, stdout=out, stderr=err, text=True, encoding="utf-8")
        if os.name == "nt":
            proc.wait()
            rss = peak_working_set(proc)
        else:
            _, status, usage = os.wait4(proc.pid, 0)
            proc.returncode = os.waitstatus_to_exitcode(status)
            rss = usage.ru_maxrss if sys.platform == "darwin" else usage.ru_maxrss * 1024
        out.seek(0)
        err.seek(0)
        stdout, stderr = out.read(), err.read()
    if proc.returncode != 0 or re.search(r"^panic:", stdout + stderr, re.M):
        sys.stderr.write(stdout[-3000:] + stderr[-3000:])
        raise SystemExit(f"run failed: {' '.join(cmd)}")
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
        if "checksum" in kv:
            row["checksum"] = int(kv["checksum"])
        if "counters" in kv:
            row["counters"] = {k: int(v) for k, v in (c.split(":", 1) for c in kv["counters"].split(",") if ":" in c)}
        yield row


def order_for(round_index: int, arms: list[str]) -> list[str]:
    rotated = arms[round_index % len(arms):] + arms[:round_index % len(arms)]
    return rotated if (round_index // len(arms)) % 2 == 0 else list(reversed(rotated))


def fmt(v: float) -> str:
    return f"{v:,.2f}" if v < 100 else f"{v:,.0f}"


def print_table(summary: dict, peak: dict, arms: list[str]) -> None:
    head = f"{'bench':<22}" + "".join(f"{arm + ' ns/op [95% CI]':>34}" for arm in arms)
    if "current" in arms and "baseline" in arms:
        head += f"{'cur/base [95% CI] decision':>39}"
    if "current" in arms and "csharp" in arms:
        head += f"{'cur/C# [95% CI] decision':>39}"
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
                comparison = row["comparisons"].get(other)
                cell = (f"{comparison['ratio']:.2f} [{comparison['ci_low']:.2f}, {comparison['ci_high']:.2f}] "
                        f"{comparison['decision']}" if comparison else "-")
                line += f"{cell:>39}"

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
    if args.counters and (args.gate or args.save_baseline or args.arms != ["current"]):
        raise ValueError("counter runs require --arms current and cannot establish latency comparisons")
    if args.gate and args.save_baseline:
        raise ValueError("choose either reference creation or gating")
    if args.warmup < 1 or args.rounds < 1 or args.passes < 1 or not 0 <= args.input_seed <= 2147483647:
        raise ValueError("invalid warmup, round, pass count or input seed")
    reference = None
    if args.target == "wasm" and "csharp" in args.arms:
        raise ValueError("wasm comparisons use Dream arms only; select --arms current or baseline current")
    if not 0 <= args.regression_threshold < 1:
        raise ValueError("invalid regression threshold")
    args.benchmark_source = args.benchmark.read_bytes()
    args.csharp_source = (CSHARP / "Program.cs").read_bytes()
    args.csharp_project = (CSHARP / "DreamBench.csproj").read_bytes()
    compatibility = bench_gate.identity(args.benchmark, CSHARP / "Program.cs", args.runner_id,
                                        args.input_seed, args.passes, args.target, args.benchmark_source)
    if args.gate or args.save_baseline:
        if os.environ.get("DREAM_NATIVE_SANITIZE"):
            raise ValueError("sanitizer instrumentation cannot form a timing reference or gate")
        if args.counters or args.filter:
            raise ValueError("references and gates require complete, uninstrumented, valid measurements")
        if not args.runner_id or not 10 <= args.rounds <= 20 or args.passes < 5 or "current" not in args.arms:
            raise ValueError("references and gates require a runner id, 10-20 rounds, five passes and the current arm")
        if args.gate:
            reference, control = bench_gate.load(args.gate, compatibility)
            if "baseline" not in args.arms: args.arms.insert(0, "baseline")
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    commands = {}
    identities = tool_versions(args)
    if reference:
        identities.update({key: value for key, value in reference['tools'].items() if key.startswith('baseline_')})
        if args.target == "wasm":
            for key in ("node_sha256", "node_version", "wasm_opt_sha256", "wasm_opt_version"):
                if identities.get(key) != reference["tools"].get(key):
                    raise ValueError(f"incompatible reference tool: {key}")
        for key in ('llvm', *(f'{name}_{suffix}' for name in ('clang', 'opt', 'llc', 'llvm-link') for suffix in ('sha256', 'version'))):
            if identities.get(key) != reference['tools'].get(key):
                raise ValueError(f'incompatible reference tool: {key}')
    for arm in args.arms:
        if arm == "baseline" and reference:
            commands[arm] = control
        elif arm == "csharp":
            commands[arm] = prepare_csharp(out, args)
        else:
            exe = args.current_dream if arm == "current" else args.baseline_dream
            commands[arm] = prepare_dream(arm, exe, out, args)
    for arm, command in commands.items():
        artifact = (Path(command[-1]).with_suffix(".wasm") if args.target == "wasm"
                    else Path(command[-1]) if arm == "csharp" else Path(command[0]))
        identities[f'{arm}_artifact_sha256'] = bench_gate.digest(artifact)
        if args.target == "wasm":
            identities[f"{arm}_runtime_sha256"] = bench_gate.digest(artifact.with_suffix(".node.runtime.mjs"))
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
    summary, peak = bench_stats.summarize(samples, rss, args.arms, args.passes, args.rounds,
                                         args.regression_threshold)
    if args.filter:
        summary = {n: r for n, r in summary.items() if n in args.filter}
    (out / "summary.json").write_text(json.dumps(
        {"tools": identities, "passes": args.passes, "rounds": args.rounds,
         "target": args.target, "instrumented": args.counters, "identity": compatibility, "regression_threshold": args.regression_threshold,
         "peak_rss": peak, "benchmarks": summary}, indent=2))
    print_table(summary, peak, args.arms)
    print(f"\nraw samples: {raw_path}\nsummary: {out / 'summary.json'}")
    status = 0
    tiny = sorted({s["name"] for s in samples if s["arm"] != "csharp"
                   and (s.get("ns_total") == 0 or s["ns_per_op"] < MIN_NS_PER_OP)})
    if tiny:
        print("SUB-TIMER rows cannot substantiate a speed claim: " + ", ".join(tiny))
        # A single arm collects diagnostics; comparisons must reject suspect timings.
        status = int(len(args.arms) > 1)
    if args.save_baseline:
        if tiny:
            raise ValueError("invalid timing rows cannot form a reference")
        arm = "baseline" if "baseline" in args.arms else "current"
        bench_gate.save(args.save_baseline, ROOT, compatibility, commands[arm], samples, arm,
                        args.passes, args.rounds, rss, identities)
    if args.gate:
        if tiny:
            raise ValueError("invalid timing rows cannot pass a gate")
        status, decisions = bench_gate.evaluate(samples, reference, args.passes, args.rounds, rss, args.regression_threshold)
        (out / "gate.json").write_text(json.dumps(decisions, indent=2) + "\n")
        for name, decision in decisions.items():
            print(f"GATE {name}: {decision['decision']} ({decision['ratio']:.3f}x)")
    return status


if __name__ == "__main__":
    try:
        if len(sys.argv) > 1 and sys.argv[1] in ("build", "generators"):
            command = sys.argv.pop(1)
            if command == "build":
                from benchmarks import build
                sys.exit(build.main())
            from benchmarks import generators
            sys.exit(generators.main())
        sys.exit(main())
    except ValueError as error:
        sys.stderr.write(f"invalid benchmark measurement: {error}\n")
        sys.exit(3)

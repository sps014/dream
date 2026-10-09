"""Process-level benchmark statistics and cross-language result validation."""
import math
import random
import statistics

from . import gate as bench_gate


# These public APIs differ in lifecycle or engine, even when results agree.
COMPARABILITY = {
    "binary_trees": "Dream completes ARC reclamation; C# may defer collection",
    "binary_trees_reclaim": "C# forces whole-heap collections; Dream reclaims each tree",
    "weak_tree": "weak registration and reclamation contracts differ",
    "regex_find": "PCRE2 JIT versus interpreted .NET Regex",
    "substring": "Dream owns a retained slice; C# copies its payload",
    "fib_rec": "LLVM proves fib pure and merges the repeated fib(n-3) call; C# executes every call",
}


def processes(samples, arm, passes, rounds):
    selected = [s for s in samples if s["arm"] == arm]
    values = bench_gate.process_samples(selected, arm, passes, rounds)
    checksums = {}
    iterations = {}
    for sample in selected:
        if type(sample.get("checksum")) is not int:
            raise ValueError(f"{arm}: missing result checksum: {sample['name']}")
        if type(sample.get("iters")) is not int or sample["iters"] <= 0:
            raise ValueError(f"{arm}: invalid iteration count: {sample['name']}")
        name = sample["name"]
        checksums.setdefault(name, set()).add(sample["checksum"])
        iterations.setdefault(name, set()).add(sample["iters"])
    if any(len(v) != 1 for v in checksums.values()):
        raise ValueError(f"{arm}: result checksum changed between passes/processes")
    if any(len(v) != 1 for v in iterations.values()):
        raise ValueError(f"{arm}: iteration count changed between passes/processes")
    return values, {n: next(iter(v)) for n, v in checksums.items()}, iterations


def summarize(samples, rss, arms, passes, rounds, threshold=0.05):
    processed = {}
    results = {}
    counts = {}
    for arm in arms:
        processed[arm], results[arm], counts[arm] = processes(samples, arm, passes, rounds)
        bench_gate.validate_memory(rss[arm], rounds)
    names = set().union(*(set(p) for p in processed.values()))
    if "baseline" in arms and "current" in arms:
        if processed["baseline"].keys() != processed["current"].keys():
            raise ValueError("Dream arms have different workload inventories")
    summary = {}
    rng = random.Random(0x5eed)
    for name in sorted(names):
        row = summary[name] = {}
        for arm in arms:
            if name not in processed[arm]:
                continue
            vals = processed[arm][name]
            if not all(math.isfinite(v) and v > 0 for v in vals):
                raise ValueError(f"invalid timings for {name}")
            lo, hi = bench_gate.interval(vals, rng)
            counters = next((s["counters"] for s in samples
                             if s["arm"] == arm and s["name"] == name and "counters" in s), None)
            row[arm] = {"median": statistics.median(vals), "ci_low": lo, "ci_high": hi,
                        "processes": rounds, "samples": passes * rounds,
                        "checksum": results[arm][name], "process_medians": vals,
                        **({"counters": counters} if counters else {})}
        comparisons = row["comparisons"] = {}
        for control in ("baseline", "csharp"):
            if "current" not in row or control not in row:
                continue
            if results["current"][name] != results[control][name]:
                raise ValueError(f"{name}: current/{control} checksum mismatch")
            if counts["current"][name] != counts[control][name]:
                raise ValueError(f"{name}: current/{control} iteration count mismatch")
            ratios = [a / b for a, b in zip(processed["current"][name], processed[control][name], strict=True)]
            lo, hi = bench_gate.interval(ratios, rng)
            comparable = control == "baseline" or name not in COMPARABILITY
            decision = ("win" if hi < 1 else "loss" if lo > 1 else "inconclusive")
            if control == "baseline":
                decision = "regression" if lo > 1 + threshold else "pass" if hi <= 1 + threshold else "inconclusive"
            comparisons[control] = {
                "ratio": statistics.median(ratios), "ci_low": lo, "ci_high": hi,
                "decision": decision if comparable else "different-contract",
                "comparable": comparable, "improvement": hi < 1,
                **({"reason": COMPARABILITY[name]} if not comparable else {}),
            }
    return summary, {arm: max(rss[arm]) for arm in arms}

"""Process-level benchmark statistics and current/baseline result validation."""
import math
import random
import statistics

from . import gate as bench_gate


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


def summarize(samples, rss, arms, passes, rounds, threshold=0.10):
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
        if "current" in row and "baseline" in row:
            if results["current"][name] != results["baseline"][name]:
                raise ValueError(f"{name}: current/baseline checksum mismatch")
            if counts["current"][name] != counts["baseline"][name]:
                raise ValueError(f"{name}: current/baseline iteration count mismatch")
            ratios = [a / b for a, b in zip(processed["current"][name], processed["baseline"][name], strict=True)]
            lo, hi = bench_gate.interval(ratios, rng)
            comparisons["baseline"] = {
                "ratio": statistics.median(ratios), "ci_low": lo, "ci_high": hi,
                "decision": "regression" if lo > 1 + threshold else "pass" if hi <= 1 + threshold else "inconclusive",
                "improvement": hi < 1,
            }
    return summary, {arm: max(rss[arm]) for arm in arms}

"""Validated benchmark references and process-paired regression decisions."""
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import random
import re
import shutil
import statistics
import subprocess
import tempfile

VERSION = 2


def digest(path):
    with Path(path).open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def identity(bench, csharp, runner, seed, passes):
    model = platform.processor()
    if platform.system() == 'Darwin':
        model = subprocess.check_output(['sysctl', '-n', 'hw.model', 'machdep.cpu.brand_string'], text=True).strip()
    elif platform.system() == 'Linux':
        model = next((line.split(':', 1)[1].strip() for line in Path('/proc/cpuinfo').read_text().splitlines()
                      if line.startswith('model name') or line.startswith('Hardware')), model)
    return {
        "benchmark": digest(bench), "csharp": digest(csharp),
        "harness": {name: digest(Path(__file__).with_name(name))
                    for name in ('bench-compare.py', 'bench_gate.py')},
        "hardware": {"platform": platform.platform(), "machine": platform.machine(),
                     "processor": model, "cpus": os.cpu_count(), "runner": runner},
        "seed": seed, "passes": passes, "profile": "Release", "counters": False,
        "names": sorted(set(re.findall(r'report\("([a-z0-9_]+)"', Path(bench).read_text())) | {"binary_trees", "binary_trees_reclaim"}),
    }


def process_samples(samples, arm, passes, rounds):
    if not isinstance(samples, list):
        raise ValueError("benchmark samples must be a list")
    groups = {}
    sinks = set()
    for sample in samples:
        if not isinstance(sample, dict) or not isinstance(sample.get("arm"), str):
            raise ValueError("invalid benchmark sample")
        if sample["arm"] != arm:
            continue
        value = sample.get("ns_per_op")
        total = sample.get("ns_total")
        if (type(value) not in (int, float) or type(total) not in (int, float)
                or not isinstance(sample.get("name"), str)
                or not re.fullmatch('[a-z][a-z0-9_]*', sample["name"])
                or type(sample.get("round")) is not int
                or type(sample.get("pass_index")) is not int):
            raise ValueError("invalid benchmark sample fields")
        if type(sample.get("sink")) is not int:
            raise ValueError("missing result sink")
        sinks.add(sample["sink"])
        if not math.isfinite(value) or not math.isfinite(total) or value <= 0 or total <= 0:
            raise ValueError(f"invalid or zero-time sample: {sample['name']}")
        key = (sample["name"], sample["round"])
        entries = groups.setdefault(key, {})
        index = sample["pass_index"]
        if index in entries:
            raise ValueError(f"duplicate sample: {key}, pass {index}")
        entries[index] = value
    names = {name for name, _ in groups}
    if not names:
        raise ValueError(f"no benchmark samples for {arm}")
    if len(sinks) != 1:
        raise ValueError("result sink changed between processes")
    result = {}
    for name in sorted(names):
        result[name] = []
        for round_index in range(rounds):
            entries = groups.get((name, round_index), {})
            if set(entries) != set(range(passes)):
                raise ValueError(f"incomplete samples: {name}, round {round_index}")
            result[name].append(statistics.median(entries.values()))
    if len(groups) != len(names) * rounds:
        raise ValueError("unexpected round indices")
    return result


def interval(values, rng):
    medians = sorted(statistics.median(rng.choices(values, k=len(values))) for _ in range(4000))
    return medians[100], medians[3899]


def validate_memory(values, rounds):
    if len(values) != rounds or any(type(x) is not int or x <= 0 for x in values):
        raise ValueError("invalid or incomplete memory samples")


def validate_rows(processed, compatibility):
    if set(processed) != set(compatibility["names"]):
        raise ValueError("missing or unexpected benchmark rows")


def validate_tools(tools, arm):
    hashes = {f'{name}_sha256' for name in ('clang', 'opt', 'llc', 'llvm-link', 'current', arm)}
    hashes.add(f'{arm}_artifact_sha256')
    if any(not isinstance(tools.get(key), str) or not re.fullmatch('[0-9a-f]{64}', tools[key]) for key in hashes):
        raise ValueError('missing or invalid tool/artifact identity')
    for key in ('llvm', *(f'{name}_version' for name in ('clang', 'opt', 'llc', 'llvm-link'))):
        if not isinstance(tools.get(key), str) or tools[key] in ('', 'unknown'):
            raise ValueError('missing tool version')


def save(path, root, compatibility, command, samples, arm, passes, rounds, rss, tools):
    if rounds < 10 or passes < 5:
        raise ValueError("references require at least ten rounds and five passes")
    processed = process_samples(samples, arm, passes, rounds)
    validate_rows(processed, compatibility)
    validate_memory(rss[arm], rounds)
    validate_tools(tools, arm)
    binary = Path(command[0])
    binary_hash = digest(binary)
    snapshot = root / "target/perf-references" / binary_hash / "benchmark.bin"
    snapshot.parent.mkdir(parents=True, exist_ok=True)
    if not snapshot.exists():
        with tempfile.NamedTemporaryFile(dir=snapshot.parent, delete=False) as temporary:
            temporary_binary = Path(temporary.name)
        try:
            shutil.copy2(binary, temporary_binary)
            with temporary_binary.open("rb") as source:
                os.fsync(source.fileno())
            os.replace(temporary_binary, snapshot)
        finally:
            temporary_binary.unlink(missing_ok=True)
    if digest(snapshot) != binary_hash:
        raise ValueError("corrupt reference binary")
    rng = random.Random(0x5eed)
    record = {"version": VERSION, "identity": compatibility,
              "reference": {"binary": str(snapshot.resolve()), "sha256": binary_hash},
              "tools": tools, "rounds": rounds,
              "samples": [s for s in samples if s["arm"] == arm], "arm": arm,
              "confidence_intervals": {name: interval(values, rng) for name, values in processed.items()},
              "peak_rss": rss[arm]}
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(mode="w", dir=path.parent, delete=False) as temporary:
        temporary.write(json.dumps(record, indent=2, allow_nan=False) + "\n")
        temporary.flush()
        os.fsync(temporary.fileno())
        temporary_path = temporary.name
    try:
        os.replace(temporary_path, path)
    finally:
        Path(temporary_path).unlink(missing_ok=True)


def load(path, compatibility):
    try:
        record = json.loads(Path(path).read_text())
        if record["version"] != VERSION or record["identity"] != compatibility:
            raise ValueError("incompatible benchmark reference")
        if record["rounds"] < 10 or compatibility["passes"] < 5:
            raise ValueError("incomplete benchmark reference")
        processed = process_samples(record["samples"], record["arm"], compatibility["passes"], record["rounds"])
        validate_rows(processed, compatibility)
        if set(record["confidence_intervals"]) != set(processed):
            raise ValueError("missing reference confidence intervals")
        rng = random.Random(0x5eed)
        for name, values in processed.items():
            if record["confidence_intervals"][name] != list(interval(values, rng)):
                raise ValueError("invalid reference confidence intervals")
        validate_memory(record["peak_rss"], record["rounds"])
        validate_tools(record['tools'], record['arm'])
        binary = record["reference"]["binary"]
        if digest(binary) != record["reference"]["sha256"]:
            raise ValueError("reference binary integrity failure")
        return record, [binary]
    except (KeyError, TypeError, OSError, json.JSONDecodeError) as error:
        raise ValueError(f"invalid benchmark reference: {error}") from error


def evaluate(samples, reference, passes, rounds, rss, threshold):
    validate_memory(rss["current"], rounds)
    validate_memory(rss["baseline"], rounds)
    current = process_samples(samples, "current", passes, rounds)
    control = process_samples(samples, "baseline", passes, rounds)
    sinks = {arm: next(s["sink"] for s in samples if s["arm"] == arm) for arm in ("current", "baseline")}
    if sinks["current"] != sinks["baseline"] or sinks["baseline"] != reference["samples"][0]["sink"]:
        raise ValueError("current/reference result sink mismatch")
    expected = set(reference["confidence_intervals"])
    if set(current) != expected or set(control) != expected:
        raise ValueError("missing or unexpected benchmark rows")
    rng = random.Random(0x5eed)
    results = {}
    status = 0
    for name in sorted(expected):
        ratios = [a / b for a, b in zip(current[name], control[name], strict=True)]
        low, high = interval(ratios, rng)
        decision = "regression" if low > 1 + threshold else "pass" if high <= 1 + threshold else "inconclusive"
        results[name] = {"ratio": statistics.median(ratios), "ci_low": low, "ci_high": high, "decision": decision}
        if decision == "regression":
            status = 1
        elif decision == "inconclusive" and status != 1:
            status = 2
    if max(rss["current"]) > max(rss["baseline"]) * (1 + threshold):
        status = 1
        results["peak_rss"] = {"decision": "regression", "ratio": max(rss["current"]) / max(rss["baseline"])}
    return status, results

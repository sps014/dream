#!/usr/bin/env python3
"""Compare warm compiler/toolchain invocations on an identical generated large sample."""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import statistics
import subprocess
import sys
import tempfile
import time


def positive_int(value: str) -> int:
    number = int(value)
    if number <= 0:
        raise argparse.ArgumentTypeError("must be positive")
    return number


def ratio(value: str) -> float:
    number = float(value)
    if not math.isfinite(number) or number < 1:
        raise argparse.ArgumentTypeError("must be a finite ratio >= 1")
    return number


def sample(functions: int) -> str:
    parts = ['import system;\nfun identity<T>(value: T): T { return value; }\n']
    for index in range(functions):
        incoming = 'identity<int>(seed)' if index == 0 else f'stage_{index - 1}(seed)'
        parts.append(f'''fun stage_{index}(seed: int): int {{
    let value = {incoming};
    for (let i = 0; i < 32; i++) {{
        if (i % 3 == 0) {{ value += i; }} else {{ value -= i; }}
    }}
    return value;
}}
''')
    parts.append(f'fun main(): void {{ System.println(stage_{functions - 1}(7)); }}\n')
    return ''.join(parts)


def measure(compiler: Path, source: Path, output: Path) -> dict:
    # Missing artifacts force a fresh compile while preserving warmed runtime/toolchain caches.
    shutil.rmtree(output, ignore_errors=True)
    output.mkdir()
    with (output / 'stdout.log').open('wb') as stdout, (output / 'stderr.log').open('wb') as stderr:
        start = time.perf_counter()
        process = subprocess.Popen(
            [str(compiler), '--emit-llvm', str(source), '-o', str(output / 'sample.ll')],
            stdout=stdout, stderr=stderr,
        )
        _, status, usage = os.wait4(process.pid, 0)
        process.returncode = os.waitstatus_to_exitcode(status)
        elapsed = time.perf_counter() - start
    if process.returncode != 0:
        raise RuntimeError((output / 'stderr.log').read_text(errors='replace'))
    peak_bytes = usage.ru_maxrss * (1 if sys.platform == 'darwin' else 1024)
    if peak_bytes <= 0:
        raise RuntimeError('OS did not supply a resident high-water mark')
    return {'wall_seconds': elapsed, 'peak_resident_bytes': peak_bytes}


def compare(baseline: list[dict], candidate: list[dict], time_ratio: float, memory_ratio: float) -> dict:
    metrics = {}
    for key, limit in [('wall_seconds', time_ratio), ('peak_resident_bytes', memory_ratio)]:
        before = statistics.median(row[key] for row in baseline)
        after = statistics.median(row[key] for row in candidate)
        if before <= 0:
            raise ValueError(f'invalid baseline {key}: {before}')
        metrics[key] = {'baseline_median': before, 'candidate_median': after,
                        'ratio': after / before, 'limit_ratio': limit, 'passed': after / before <= limit}
    return metrics


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', type=Path, required=True)
    parser.add_argument('--candidate', type=Path, required=True)
    parser.add_argument('--baseline-revision')
    parser.add_argument('--candidate-revision')
    parser.add_argument('--functions', type=positive_int, default=200)
    parser.add_argument('--runs', type=positive_int, default=5)
    parser.add_argument('--time-ratio', type=ratio, default=1.25)
    parser.add_argument('--memory-ratio', type=ratio, default=1.20)
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    if not hasattr(os, 'wait4'):
        parser.error('this benchmark requires Unix wait4 resource accounting')
    compilers = {'baseline': args.baseline.resolve(), 'candidate': args.candidate.resolve()}
    readings: dict[str, list[dict]] = {label: [] for label in compilers}
    text = sample(args.functions)
    with tempfile.TemporaryDirectory(prefix='dream-compile-budget-') as temporary:
        root = Path(temporary)
        source = root / 'sample.dream'
        source.write_text(text)
        for compiler in compilers.values():
            measure(compiler, source, root / 'out')
        for iteration in range(args.runs):
            order = list(compilers) if iteration % 2 == 0 else list(reversed(compilers))
            for label in order:
                reading = measure(compilers[label], source, root / 'out')
                readings[label].append(reading)
                print(f'{label} {iteration + 1}: {reading}', flush=True)
    metrics = compare(readings['baseline'], readings['candidate'], args.time_ratio, args.memory_ratio)
    report = {'scope': 'compiler and waited-for toolchain children; peak is largest resident high-water mark, not concurrent tree RSS',
              'functions': args.functions, 'source_sha256': hashlib.sha256(text.encode()).hexdigest(),
              'compilers': {key: str(value) for key, value in compilers.items()},
              'revisions': {'baseline': args.baseline_revision, 'candidate': args.candidate_revision},
              'readings': readings, 'metrics': metrics}
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(metrics, indent=2))
    return 0 if all(metric['passed'] for metric in metrics.values()) else 1


if __name__ == '__main__':
    raise SystemExit(main())

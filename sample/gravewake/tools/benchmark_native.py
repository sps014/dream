"""Measure sequential native runs; all artifacts stay in the ignored build cache."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess

ROOT = Path(__file__).resolve().parents[3]
OUTPUT = ROOT / 'target/gravewake-assets/native-measurements.json'
CASES = {
    'standard': ['--benchmark'],
    'low': ['--benchmark-low'],
    'high': ['--benchmark-high'],
    'ultra': ['--benchmark-ultra'],
    'high-combat': ['--benchmark-high', '--benchmark-combat'],
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exe', type=Path, default=ROOT / 'sample/gravewake/target/release/main.bin')
    parser.add_argument('--case', choices=list(CASES), action='append')
    parser.add_argument('--rt', action='store_true', help='Enable optional hardware RT')
    args = parser.parse_args()
    output = OUTPUT.with_name('native-rt-measurements.json' if args.rt else 'native-raster-measurements.json')
    env = dict(os.environ)
    env['PATH'] = str(ROOT / 'target/debug') + os.pathsep + env.get('PATH', '')
    report = {
        'utc': datetime.now(timezone.utc).isoformat(), 'os': platform.platform(),
        'binarySha256': hashlib.sha256(args.exe.read_bytes()).hexdigest(),
        'manifestSha256': hashlib.sha256((ROOT / 'sample/gravewake/assets/packed/manifest.json').read_bytes()).hexdigest(),
        'measurement': 'Whole frame loop, 30 warmup frames, 180 measured intervals, vsync off; not GPU timestamps',
        'rtEnabled': args.rt, 'runs': [],
    }
    for name in args.case or CASES:
        result = subprocess.run([str(args.exe), *CASES[name], *(['--rt'] if args.rt else [])], cwd=ROOT, env=env,
                                capture_output=True, text=True, timeout=120, check=True)
        match = re.search(r'BENCHMARK .*?: ([\d.]+) ms/frame, (\d+) frames \| p95=([\d.]+) p99=([\d.]+)', result.stdout)
        if not match or 'GPU error:' in result.stdout:
            raise RuntimeError(name + ' did not finish:\n' + result.stdout + result.stderr)
        run = {'case': name, 'meanMs': float(match[1]), 'frames': int(match[2]),
               'p95Ms': float(match[3]), 'p99Ms': float(match[4]), 'stdout': result.stdout}
        report['runs'].append(run)
        output.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
        print(name, f"mean {run['meanMs']:.2f} ms, p95 {run['p95Ms']:.2f}, p99 {run['p99Ms']:.2f}", flush=True)
    print(output)


if __name__ == '__main__':
    main()

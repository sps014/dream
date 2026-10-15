#!/usr/bin/env python3
"""Run the stdlib v1 release gates sequentially and record reproducible per-platform evidence."""
import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]


def commands():
    checks = [
        ["cargo", "build", "--workspace"],
        ["cargo", "build", "--release", "--workspace"],
        ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
        ["cargo", "test", "--workspace", "--", "--test-threads=8"],
        [sys.executable, "scripts/build-cli-parser.py", "--check"],
        [sys.executable, "scripts/build-data-codecs.py", "--check"],
        ["node", "scripts/bundle-runtime.mjs", "--check"],
        ["node", "--test", *sorted(str(p.relative_to(ROOT)) for p in (ROOT / "scripts").glob("test-*-host.mjs")),
         "scripts/test-stdlib-hosts.mjs", "scripts/test-bundle-runtime.mjs"],
        [sys.executable, "scripts/test_corpus.py", "--both"],
        [sys.executable, "scripts/test_corpus.py", "--both", "--release"],
        [sys.executable, "-m", "unittest", "discover", "-s", "tooling/docs", "-p", "test_*.py"],
        [sys.executable, "tooling/docs/api_inventory.py"],
        [sys.executable, "-m", "mkdocs", "build", "--strict"],
        [sys.executable, "tooling/docs/check_examples.py"],
        [sys.executable, "scripts/test-http-csv-pipeline.py"],
        [sys.executable, "scripts/test-http-csv-pipeline.py", "--release"],
        ["cargo", "test", "--test", "integration", "e2e_tests::codegen_is_deterministic_full_corpus", "--", "--ignored", "--exact"],
        ["cargo", "test", "-p", "dreamer", "--test", "pack_portability", "--", "--ignored"],
    ]
    if platform.system() != "Windows":
        checks.append(["cargo", "test", "--test", "integration", "e2e_tests::native_asan_focused_goldens", "--", "--ignored", "--exact"])
    if platform.system() != "Windows":
        checks.extend([["cargo", "test", "--test", "integration", "runtime_cancellation::", "--", "--test-threads=1"],
                       ["cargo", "test", "--test", "integration", "runtime_worker::", "--", "--test-threads=1"]])
    return checks


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "target/stdlib-v1-qualification")
    parser.add_argument("--list", action="store_true", help="print planned gates without claiming qualification")
    args = parser.parse_args()
    steps = commands()
    if args.list:
        print(json.dumps(steps, indent=2))
        return 0
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    report = {
        "platform": platform.platform(), "machine": platform.machine(),
        "revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "dirty": bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).strip()),
        "started": datetime.now(timezone.utc).isoformat(), "passed": False,
        "qualification": "native-node", "browser_qualification_required": True, "steps": [],
    }
    for index, command in enumerate(steps):
        log = output / f"step-{index + 1:02d}.log"
        started = time.monotonic()
        print(f"[{index + 1}/{len(steps)}] {' '.join(command)}", flush=True)
        with log.open("w", encoding="utf-8") as stream:
            try:
                env = os.environ.copy()
                if "e2e_tests::native_asan_focused_goldens" in command:
                    env["DREAM_NATIVE_SANITIZE"] = "address" if platform.system() == "Darwin" else "address,leak"
                    env["ASAN_OPTIONS"] = "halt_on_error=1" if platform.system() == "Darwin" else "detect_leaks=1:halt_on_error=1"
                if any(case in command for case in ["runtime_cancellation::", "runtime_worker::"]):
                    env["DREAM_WORKER_SANITIZER"] = "address,undefined"
                result = subprocess.run(command, cwd=ROOT, env=env, stdout=stream, stderr=subprocess.STDOUT, timeout=7200, check=False)
                code = result.returncode
            except (OSError, subprocess.TimeoutExpired) as error:
                stream.write(str(error) + "\n")
                code = -1
        report["steps"].append({"command": command, "exit_code": code, "seconds": round(time.monotonic() - started, 3), "log": log.name})
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        if code:
            print(f"FAIL (see {log})", flush=True)
            return 1
        print("PASS", flush=True)
    report["passed"] = True
    report["finished"] = datetime.now(timezone.utc).isoformat()
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Qualify phase 3–4 I/O and networking on the current native and Node hosts."""
import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import platform
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "target/phase34-qualification")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    commands = [
        ["cargo", "test", "-p", "dream-host-net"],
        ["cargo", "test", "--test", "integration", "runtime_cancellation::"],
        ["cargo", "test", "--test", "integration", "runtime_file_operation::"],
        ["cargo", "test", "--test", "integration", "http_client::", "--", "--nocapture"],
        ["node", "--test", "scripts/test-cancellation-host.mjs", "scripts/test-http-host.mjs",
         "scripts/test-stdlib-hosts.mjs", "scripts/test-bundle-runtime.mjs"],
    ]
    for optimized in [False, True]:
        for case in ["event_cancellation", "async_file_lifecycle", "url_basics", "http_types"]:
            commands.append([sys.executable, "scripts/test_corpus.py", "--both",
                             *(["--release"] if optimized else []), case])
    report = {
        "platform": platform.platform(), "machine": platform.machine(),
        "revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "dirty": bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).strip()),
        "started": datetime.now(timezone.utc).isoformat(), "steps": [],
    }
    failed = False
    for index, command in enumerate(commands):
        log = output / f"step-{index + 1:02d}.log"
        started = time.monotonic()
        print(f"[{index + 1}/{len(commands)}] {' '.join(command)}", flush=True)
        with log.open("w", encoding="utf-8") as stream:
            try:
                result = subprocess.run(command, cwd=ROOT, stdout=stream, stderr=subprocess.STDOUT,
                                        timeout=1200, check=False)
                code = result.returncode
            except (OSError, subprocess.TimeoutExpired) as error:
                stream.write(str(error) + "\n")
                code = -1
        report["steps"].append({"command": command, "exit_code": code,
                                "seconds": round(time.monotonic() - started, 3), "log": log.name})
        failed |= code != 0
        print("PASS" if code == 0 else f"FAIL (see {log})", flush=True)
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    report["passed"] = not failed
    report["finished"] = datetime.now(timezone.utc).isoformat()
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    return int(failed)


if __name__ == "__main__":
    sys.exit(main())

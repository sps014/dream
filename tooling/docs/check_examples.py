"""Run the complete documentation examples listed in example-cases.json."""

import argparse
import json
import os
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dream", type=Path, default=ROOT / "target/debug/dream.exe" if os.name == "nt" else ROOT / "target/debug/dream")
    args = parser.parse_args()
    cases = json.loads((ROOT / "tooling/docs/example-cases.json").read_text(encoding="utf-8"))
    output = ROOT / "target/docs-examples"
    output.mkdir(parents=True, exist_ok=True)
    failed = []
    for case in cases:
        page = (ROOT / case["page"]).read_text(encoding="utf-8")
        blocks = re.findall(r"^```dream\s*\n(.*?)^```\s*$", page, re.M | re.S)
        if case["block"] >= len(blocks):
            failed.append(case["name"])
            print(f"FAIL {case['name']}: example block is missing from {case['page']}")
            continue
        source = blocks[case["block"]]
        directory = output / case["name"]
        directory.mkdir(exist_ok=True)
        path = directory / "main.dream"
        path.write_text(source, encoding="utf-8")
        try:
            result = subprocess.run([str(args.dream.resolve()), "run", str(path)], cwd=directory,
                                    capture_output=True, text=True, timeout=120)
        except (OSError, subprocess.TimeoutExpired) as error:
            failed.append(case["name"])
            print(f"FAIL {case['name']}: {error}")
            continue
        stdout = result.stdout.replace("\r\n", "\n")
        if result.returncode != 0 or stdout != case["stdout"]:
            failed.append(case["name"])
            print(f"FAIL {case['name']} ({case['page']}): exit {result.returncode}\n{result.stdout}\n{result.stderr}")
        else:
            print(f"PASS {case['name']}")
    if failed:
        raise SystemExit(f"{len(failed)} documentation examples failed.")
    print(f"{len(cases)} documentation examples passed.")


if __name__ == "__main__":
    main()

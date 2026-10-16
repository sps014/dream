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
    parser.add_argument("--filter", help="Only check examples whose names contain this text")
    args = parser.parse_args()
    cases = json.loads((ROOT / "tooling/docs/example-cases.json").read_text(encoding="utf-8"))
    if args.filter:
        cases = [case for case in cases if args.filter in case["name"]]
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
        source = "\n".join(blocks[index] for index in [case["block"], *case.get("append_blocks", [])])
        directory = output / case["name"]
        directory.mkdir(exist_ok=True)
        path = directory / case.get("entry", "main.dream")
        path.write_text(source, encoding="utf-8")
        for extra in case.get("files", []):
            destination = directory / extra["name"]
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_text(blocks[extra["block"]], encoding="utf-8")
        mode = case.get("mode", "run")
        if mode not in {"run", "compile", "test"}:
            raise ValueError(f"Unsupported example mode: {mode}")
        command = [str(args.dream.resolve()), "--emit-llvm", str(path)] if mode == "compile" else [str(args.dream.resolve()), mode, str(path)]
        try:
            result = subprocess.run(command, cwd=directory,
                                    capture_output=True, text=True, timeout=case.get("timeout", 120))
        except (OSError, subprocess.TimeoutExpired) as error:
            failed.append(case["name"])
            print(f"FAIL {case['name']}: {error}")
            continue
        stdout = result.stdout.replace("\r\n", "\n")
        if result.returncode != 0 or (mode == "run" and stdout != case["stdout"]):
            failed.append(case["name"])
            print(f"FAIL {case['name']} ({case['page']}): exit {result.returncode}\n{result.stdout}\n{result.stderr}")
        else:
            print(f"PASS {case['name']}")
    if failed:
        raise SystemExit(f"{len(failed)} documentation examples failed.")
    print(f"{len(cases)} documentation examples passed.")


if __name__ == "__main__":
    main()

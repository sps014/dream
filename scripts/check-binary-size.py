#!/usr/bin/env python3
"""Check a size-optimized hello executable and its release core host library."""

import argparse
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile


# Raw distribution bytes, not stripped development builds or filesystem block usage.
# Separate artifact limits prevent one shrinking artifact from hiding another's growth.
BUDGETS = {"hello": 96 * 1024, "core": 3 * 1024 * 1024}


def check_budget(sizes):
    failures = []
    for name, limit in BUDGETS.items():
        size = sizes[name]
        if size <= 0 or size > limit:
            failures.append(f"{name}: {size} bytes, allowed 1..{limit}")
    return failures


def measure(compiler, host_library):
    root = Path(__file__).resolve().parent.parent
    with tempfile.TemporaryDirectory(prefix="dream-size-") as temporary:
        directory = Path(temporary)
        isolated_compiler = directory / "dream"
        shutil.copy2(compiler, isolated_compiler)
        # Discovery follows the compiler's real path, so copy instead of symlinking.
        shutil.copy2(host_library, directory / host_library.name)
        package = directory / "package"
        package.mkdir()
        output = package / "hello.ll"
        subprocess.run(
            [str(isolated_compiler), "-Os", "--relocatable", "-o", str(output),
             str(root / "tests/size/hello.dream")],
            check=True,
        )
        manifest = json.loads(output.with_suffix(".abi.json").read_text())
        if manifest["host_capabilities"] != ["core"]:
            raise RuntimeError("hello unexpectedly requires optional host capabilities")
        binary = output.with_suffix(".bin")
        environment = os.environ.copy()
        environment.pop("DYLD_LIBRARY_PATH", None)
        environment.pop("LD_LIBRARY_PATH", None)
        result = subprocess.run([str(binary)], env=environment, capture_output=True,
                                text=True, check=True)
        if result.stdout.strip() != "hello size budget":
            raise RuntimeError(f"unexpected hello output: {result.stdout!r}")
        return {"hello": binary.stat().st_size, "core": host_library.stat().st_size}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", type=Path, required=True)
    parser.add_argument("--host-library", type=Path, required=True,
                        help="core shared library from cargo build --release -p dream-host-core")
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    sizes = measure(args.compiler.resolve(strict=True), args.host_library.resolve(strict=True))
    failures = check_budget(sizes)
    report = {"platform": platform.system(), "architecture": platform.machine(),
              "guest_opt": "Os", "host_profile": "release", "bytes": sizes,
              "budgets": BUDGETS, "failures": failures}
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    if failures:
        raise SystemExit("binary size budget exceeded: " + "; ".join(failures))


if __name__ == "__main__":
    main()

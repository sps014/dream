#!/usr/bin/env python3
"""Check host-free Hello World imports and report release capability sizes."""

import argparse
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile


MEASURED_CAPABILITIES = ("core", "unicode", "crypto", "process", "timezone")


def measure(compiler, host_directory, llvm_directory):
    root = Path(__file__).resolve().parent.parent
    with tempfile.TemporaryDirectory(prefix="dream-size-") as temporary:
        directory = Path(temporary)
        isolated_compiler = directory / ("dream.exe" if os.name == "nt" else "dream")
        shutil.copy2(compiler, isolated_compiler)
        # Discovery follows the compiler's real path, so copy instead of symlinking.
        # A host-free program must compile without any capability library installed.
        package = directory / "package"
        package.mkdir()
        output = package / "hello.ll"
        compile_env = os.environ.copy()
        compile_env.update(DREAM_HOME=str(directory), DREAM_BIN=str(isolated_compiler), DREAM_TARGETS=str(directory / "targets"))
        compile_env["DREAM_LLVM"] = str(llvm_directory)
        subprocess.run(
            [str(isolated_compiler), "-O3", "--relocatable", "-o", str(output),
             str(root / "tests/size/hello.dream")],
            check=True, env=compile_env,
        )
        manifest = json.loads(output.with_suffix(".abi.json").read_text())
        if manifest["host_capabilities"] != []:
            raise RuntimeError("hello unexpectedly requires host capabilities")
        binary = output.with_suffix(".bin")
        if any("dream_host" in path.name for path in package.iterdir()):
            raise RuntimeError("host-free hello bundled a Dream library")
        if platform.system() == "Darwin":
            imports = subprocess.check_output(["otool", "-L", str(binary)], text=True)
        elif platform.system() == "Linux":
            imports = subprocess.check_output(["readelf", "-d", str(binary)], text=True)
        else:
            imports = subprocess.check_output(["dumpbin", "/imports", str(binary)], text=True)
        if "dream_host" in imports.lower():
            raise RuntimeError("host-free hello imports a Dream library")
        environment = os.environ.copy()
        environment.pop("DYLD_LIBRARY_PATH", None)
        environment.pop("LD_LIBRARY_PATH", None)
        environment["PATH"] = str(Path(os.environ["SystemRoot"]) / "System32") if os.name == "nt" else "/usr/bin:/bin"
        result = subprocess.run([str(binary)], env=environment, capture_output=True,
                                text=True, check=True)
        if result.stdout.strip() != "Hello, world!":
            raise RuntimeError(f"unexpected hello output: {result.stdout!r}")
        sizes = {"hello": binary.stat().st_size}
        for capability in MEASURED_CAPABILITIES:
            name = (f"dream_host_{capability}.dll" if os.name == "nt" else
                    f"libdream_host_{capability}.dylib" if platform.system() == "Darwin" else
                    f"libdream_host_{capability}.so")
            sizes[capability] = (host_directory / name).stat().st_size
        return sizes


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", type=Path, required=True)
    parser.add_argument("--host-directory", type=Path, required=True,
                        help="directory containing release core, unicode, crypto, process and timezone libraries")
    parser.add_argument("--llvm-directory", type=Path, required=True,
                        help="pinned LLVM bin directory available to the isolated compiler")
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    sizes = measure(args.compiler.resolve(strict=True), args.host_directory.resolve(strict=True),
                    args.llvm_directory.resolve(strict=True))
    report = {"platform": platform.system(), "architecture": platform.machine(),
              "guest_opt": "O3", "host_profile": "release", "bytes": sizes,
              "hello_bundle_bytes": sizes["hello"]}
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()

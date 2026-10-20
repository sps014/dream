#!/usr/bin/env python3
"""Build the shared C DGPB reader as a freestanding WASM capability."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
C = ROOT / "crates/dream-mir/src/runtime/c"
OUTPUT = ROOT / "tests/fixtures/gpu_parser.wasm"
METADATA = ROOT / "tests/fixtures/gpu_parser.json"
SOURCES = [C / "core/gpu_bundle.c", C / "sys/wasi/gpu_bundle_host.c", C / "core/memory.c"]
INPUTS = [*SOURCES, C / "include/dream_gpu_bundle.h", C / "include/dream_gpu_bindings.h",
          ROOT / "crates/dream-abi/src/toolchain.rs", Path(__file__)]


def fingerprint():
    digest = hashlib.sha256()
    for path in INPUTS:
        digest.update(path.relative_to(ROOT).as_posix().encode())
        digest.update(b"\0")
        digest.update(path.read_text(encoding="utf-8").encode("utf-8"))
    return digest.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    if args.check:
        metadata = json.loads(METADATA.read_text()) if METADATA.exists() else {}
        if (metadata.get("source_sha256") != fingerprint() or not OUTPUT.exists()
                or metadata.get("wasm_sha256") != hashlib.sha256(OUTPUT.read_bytes()).hexdigest()):
            raise SystemExit("GPU parser is stale; run python scripts/build-gpu-parser.py")
        print("GPU parser matches its shared C sources.")
        return
    version = re.search(r'const LLVM_VERSION: &str = "([^"]+)"', INPUTS[-2].read_text())[1]
    tool = Path(os.environ.get("DREAM_LLVM", Path.home() / f".dream/toolchains/llvm-{version}"))
    tool = tool if (tool / "clang.exe").exists() or (tool / "clang").exists() else tool / "bin"
    clang = tool / ("clang.exe" if os.name == "nt" else "clang")
    command = [str(clang), "--target=wasm32-unknown-unknown", "-std=c11", "-Oz", "-flto",
               "-mbulk-memory", "-DDREAM_WASM32", "-nostdlib", "-Wall", "-Wextra", "-Werror",
               "-I", str(C / "include"), *map(str, SOURCES), "-Wl,--no-entry", "-Wl,--strip-all",
               "-Wl,--initial-memory=131072", "-Wl,--max-memory=67108864"]
    for name in ["dream_gpu_bundle_abi_version", "gpu_bundle_alloc", "gpu_bundle_reset", "dream_gpu_bundle_index", "dream_gpu_bundle_error"]:
        command.append(f"-Wl,--export={name}")
    subprocess.run([*command, "-o", str(OUTPUT)], check=True)
    METADATA.write_text(json.dumps({"source_sha256": fingerprint(),
                                  "wasm_sha256": hashlib.sha256(OUTPUT.read_bytes()).hexdigest()}, indent=2) + "\n", encoding="utf-8", newline="\n")
    print(f"Wrote {OUTPUT.stat().st_size} bytes of shared C GPU parsing.")


if __name__ == "__main__":
    main()

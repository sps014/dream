#!/usr/bin/env python3
"""Build the shared data-format WebAssembly adapter; --check verifies its source fingerprint."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
FAMILIES = ("csv", "compression")
METADATA = ROOT / "runtime/src/hosts/data_codecs.json"


def fingerprint():
    sources = [ROOT / "Cargo.lock", ROOT / "Cargo.toml", *sorted((ROOT / "crates/dream-host-data").rglob("*.rs")),
               ROOT / "crates/dream-host-data/Cargo.toml",
               ROOT / "crates/dream-abi/src/capabilities.rs",
               ROOT / "crates/dream-abi/src/capability_types.rs", Path(__file__)]
    digest = hashlib.sha256()
    for path in sources:
        digest.update(path.relative_to(ROOT).as_posix().encode())
        digest.update(b"\0")
        digest.update(path.read_text(encoding="utf-8").encode("utf-8"))
    return digest.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    if args.check:
        metadata = json.loads(METADATA.read_text(encoding="utf-8")) if METADATA.exists() else {}
        if metadata.get("source_sha256") != fingerprint():
            raise SystemExit("Data codecs are stale; run python scripts/build-data-codecs.py")
        for family in FAMILIES:
            output = ROOT / f"runtime/dream-{family}.wasm"
            if not output.exists() or metadata.get("families", {}).get(family) != hashlib.sha256(output.read_bytes()).hexdigest():
                raise SystemExit(f"Data {family} adapter is stale; run python scripts/build-data-codecs.py")
        print("Data codec families match their Rust sources and dependency lock.")
        return
    hashes = {}
    for family in FAMILIES:
        subprocess.run(["cargo", "build", "-p", "dream-host-data", "--target", "wasm32-unknown-unknown", "--profile", "capability-wasm", "--no-default-features", "--features", family, "--locked"], cwd=ROOT, check=True)
        binary = ROOT / "target/wasm32-unknown-unknown/capability-wasm/dream_host_data.wasm"
        data = binary.read_bytes()
        output = ROOT / f"runtime/dream-{family}.wasm"
        output.write_bytes(data)
        hashes[family] = hashlib.sha256(data).hexdigest()
        print(f"Wrote {len(data)} bytes to {output.relative_to(ROOT)}.")
    compiler = subprocess.check_output(["rustc", "--version"], text=True).strip()
    METADATA.write_text(json.dumps({
        "compiler": compiler,
        "source_sha256": fingerprint(),
        "families": hashes,
    }, indent=2) + "\n", encoding="utf-8", newline="\n")


if __name__ == "__main__":
    main()

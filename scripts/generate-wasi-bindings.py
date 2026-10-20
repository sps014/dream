#!/usr/bin/env python3
"""Regenerate the pinned WASI HTTP Canonical ABI bindings."""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
VERSION = "0.63.0"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    name = "wit-bindgen.exe" if os.name == "nt" else "wit-bindgen"
    tool = Path(os.environ.get("DREAM_WIT_BINDGEN", Path.home() / ".dream/toolchains" / ("wit-bindgen-" + VERSION) / name))
    version = subprocess.check_output([str(tool), "--version"], text=True).strip()
    if not version.startswith("wit-bindgen-cli " + VERSION + " "):
        raise SystemExit("expected wit-bindgen " + VERSION + ", found " + version)
    for preview in ["p2", "p3"]:
        destination = ROOT / "crates/dream-mir/src/runtime/c/sys/wasi/component" / preview
        wit = ROOT / "crates/dream-mir/src/runtime/wit" / ("wasi" + preview)
        with tempfile.TemporaryDirectory(prefix="dream-wit-") as temporary:
            subprocess.run([str(tool), "c", str(wit), "--world", "transport", "--rename-world", "dream_http_" + preview, "--out-dir", temporary], check=True)
            for generated in sorted(Path(temporary).iterdir()):
                target = destination / generated.name
                data = generated.read_bytes()
                if generated.suffix in {".c", ".h"}:
                    data = ("\n".join(line.rstrip() for line in data.decode("utf-8").splitlines()) + "\n").encode("utf-8")
                if args.check:
                    if not target.exists() or target.read_bytes() != data:
                        raise SystemExit(str(target) + " is stale; run python scripts/generate-wasi-bindings.py")
                else:
                    target.write_bytes(data)
    print("WASI HTTP bindings match pinned WIT and wit-bindgen.")


if __name__ == "__main__":
    main()

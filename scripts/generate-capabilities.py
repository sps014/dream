#!/usr/bin/env python3
"""Generate C, Rust, Dream, and JS capability bindings from dream-abi's contract registry."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def main():
    with tempfile.TemporaryDirectory(prefix="dream-capabilities-") as temporary:
        binary = Path(temporary) / ("generate.exe" if os.name == "nt" else "generate")
        subprocess.run(["rustc", "--edition=2024", str(ROOT / "crates/dream-abi/generate_capabilities.rs"), "-o", str(binary)], cwd=ROOT, check=True)
        subprocess.run([str(binary), str(ROOT), *sys.argv[1:]], cwd=ROOT, check=True)


if __name__ == "__main__":
    main()

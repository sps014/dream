#!/usr/bin/env python3
"""Build the portable drawing host shipped beside generated Dream JavaScript runtimes."""
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def main():
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    if not target.is_absolute():
        target = ROOT / target
    if not shutil.which("wasm-bindgen"):
        raise SystemExit("Install wasm-bindgen-cli 0.2.129: cargo install wasm-bindgen-cli --version 0.2.129 --locked")
    subprocess.run(["cargo", "build", "--locked", "-p", "dream-drawing", "--target", "wasm32-unknown-unknown", "--release"], cwd=ROOT, check=True)
    output = target / "drawing-web"
    subprocess.run(["wasm-bindgen", str(target / "wasm32-unknown-unknown/release/dream_drawing.wasm"),
                    "--target", "web", "--out-dir", str(output), "--out-name", "dream_drawing"], cwd=ROOT, check=True)
    shutil.copyfile(ROOT / "crates/dream-drawing/fonts/OFL.txt", output / "NotoSans-OFL.txt")
    print(f"Drawing browser host: {output}")


if __name__ == "__main__":
    main()

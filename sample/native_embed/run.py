"""Build the sample library and run its C consumer in both link modes."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def run(*args):
    subprocess.run(args, check=True)


def main():
    source = Path(__file__).resolve().parent
    cflags = []
    if sys.platform == "darwin":
        static, shared = ".a", ".dylib"
        sdk = os.getenv("SDKROOT") or subprocess.check_output(
            ["xcrun", "--sdk", "macosx", "--show-sdk-path"], text=True
        ).strip()
        cflags = ["-isysroot", sdk]
    elif os.name == "nt":
        static, shared = ".lib", ".dll"
    else:
        static, shared = ".a", ".so"

    with tempfile.TemporaryDirectory(prefix="dream-embed-") as scratch:
        manifest = (source / "dream.toml").read_text(encoding="utf-8")
        for kind, suffix in (("staticlib", static), ("cdylib", shared)):
            root = Path(scratch) / kind
            shutil.copytree(source / "src", root / "src")
            (root / "dream.toml").write_text(
                manifest.replace('"staticlib"', f'"{kind}"'), encoding="utf-8"
            )
            stem = root / "native_embed"
            library = stem.with_suffix(suffix)
            run(os.getenv("DREAM", "dream"), "build", root / "src/native_embed.dream", "-o", library)
            link_input = library
            if kind == "staticlib":
                flags = json.loads(stem.with_suffix(".link.json").read_text(encoding="utf-8"))
                link_args = flags["link_args"]
            elif os.name == "nt":
                link_input = library.with_suffix(".lib")
                link_args = []
            else:
                link_args = [f"-Wl,-rpath,{root}"]
            exe = root / ("consumer.exe" if os.name == "nt" else "consumer")
            run(
                os.getenv("CC", "clang"), *cflags, source / "consumer.c",
                f"-I{root}", link_input, *link_args, "-o", exe,
            )
            run(exe)


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Build the complete core without target SDK headers or a hosted standard library."""
import argparse
import pathlib
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
CORE = ROOT / "crates/dream-mir/src/runtime/c/core"


def run(*args):
    result = subprocess.run([str(arg) for arg in args], text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
    if result.returncode:
        raise RuntimeError(result.stdout)
    return result.stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--clang", default="clang")
    parser.add_argument("--linker", default="ld.lld")
    args = parser.parse_args()
    runtime = CORE.parent
    for source in sorted(runtime.rglob("*.c")):
        relative = source.relative_to(runtime)
        if relative.parts[0] in ("deps", "pcre2"):
            continue
        lines = len(source.read_text(encoding="utf-8").splitlines())
        if lines > 600:
            raise RuntimeError(f"runtime unit exceeds 600 lines: {relative} ({lines})")
    resource = pathlib.Path(run(args.clang, "-print-resource-dir").strip()) / "include"
    with tempfile.TemporaryDirectory(prefix="dream-core-") as directory:
        temporary = pathlib.Path(directory)
        objects = []
        for source in sorted(CORE.glob("*.c")):
            obj = temporary / (source.stem + ".o")
            run(args.clang, "--target=x86_64-unknown-linux-gnu", "-std=gnu11", "-O1",
                "-ffreestanding", "-nostdlib", "-nostdinc", "-fno-stack-protector",
                "-isystem", resource, "-I", CORE / "include/freestanding",
                "-I", CORE / "include", "-c", source, "-o", obj)
            objects.append(obj)
        harness = temporary / "embedding.c"
        harness.write_text("""#include "dream_core.h"
#include "dream_platform_internal.h"
_Thread_local dream_ptr g0;
void *dream_ft_get(int32_t index) { (void)index; return 0; }
void dream_future_fini(dream_ptr ptr) { (void)ptr; }
void dream_release_object(dream_ptr ptr) { dream_release(ptr); }
const dream_platform dream_default_platform = {0};
int dream_core_test(void) { return 0; }
""")
        anchor = temporary / "embedding.o"
        run(args.clang, "--target=x86_64-unknown-linux-gnu", "-std=gnu11", "-O1",
            "-ffreestanding", "-nostdlib", "-nostdinc", "-fno-stack-protector",
            "-isystem", resource, "-I", CORE / "include", "-c", harness, "-o", anchor)
        linked = temporary / "dream-core"
        linker = [args.linker]
        if pathlib.Path(args.linker).stem == "lld":
            linker += ["-flavor", "gnu"]
        # Link every function, with no section collection, CRT, sys object, or libc.
        run(*linker, "--no-undefined", "--entry=dream_core_test", *objects, anchor, "-o", linked)
        print(f"freestanding core: {len(objects)} units linked; no OS/libc imports")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Build the compiler and run assertion-based Dream projects plus isolated Rust corpus tests."""
import argparse
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def gpu_environment(env):
    """GPU suites run strictly ahead of time: the host aborts if the Slang compiler is ever
    loaded, and no Slang toolchain is named at run time (the build finds the pinned one). On
    macOS, Vulkan runs through the Khronos loader and MoltenVK the host build cached, unless
    the environment already names a Vulkan driver."""
    env["DREAM_GPU_STRICT_AOT"] = "1"
    env.pop("DREAM_SLANG", None)
    if sys.platform != "darwin" or "VK_DRIVER_FILES" in env:
        return
    target = Path(env.get("CARGO_TARGET_DIR", ROOT / "target"))
    if not target.is_absolute():
        target = ROOT / target
    cache = Path(env.get("DREAM_GPU_CACHE", target / "gpu-cache"))
    loaders = sorted(cache.glob("vulkan-loader-*/**/libvulkan.1.dylib"))
    drivers = sorted(p for p in cache.glob("moltenvk-*/**/MoltenVK_icd.json") if "dynamic" in p.parts)
    if loaders and drivers:
        env["DYLD_LIBRARY_PATH"] = os.pathsep.join(
            filter(None, [str(loaders[0].parent), env.get("DYLD_LIBRARY_PATH")]))
        env["VK_DRIVER_FILES"] = str(drivers[0])
        env.setdefault("MVK_CONFIG_LOG_LEVEL", "0")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    host = parser.add_mutually_exclusive_group()
    host.add_argument("--native", action="store_true", help="native only (default)")
    host.add_argument("--node", action="store_true", help="Node only")
    host.add_argument("--both", action="store_true", help="native and Node, including isolated stdout parity")
    parser.add_argument("--concurrency", action="store_true", help="only concurrency-related case names")
    parser.add_argument("--release", action="store_true", help="optimized guest builds")
    parser.add_argument("--debug-info", action="store_true", help="include guest debugger information")
    parser.add_argument("filter", nargs="?", help="case-name substring")
    args = parser.parse_args()
    env = os.environ.copy()
    env["DREAM_CORPUS_HOSTS"] = "native,node" if args.both else "node" if args.node else "native"
    for key, value in [("DREAM_CORPUS_FILTER", args.filter),
                       ("DREAM_CORPUS_CONCURRENCY", "1" if args.concurrency else None),
                       ("DREAM_CORPUS_RELEASE", "1" if args.release else None),
                       ("DREAM_CORPUS_DEBUG_INFO", "1" if args.debug_info else None)]:
        if value is None:
            env.pop(key, None)
        else:
            env[key] = value
    gpu_environment(env)
    build = subprocess.run(["cargo", "build", "--workspace", "--quiet"], cwd=ROOT, env=env)
    if build.returncode:
        return build.returncode
    return subprocess.run([
        "cargo", "test", "-p", "dream", "--test", "integration", "corpus::run::full_corpus",
        "--", "--ignored", "--exact", "--nocapture",
    ], cwd=ROOT, env=env).returncode


if __name__ == "__main__":
    sys.exit(main())

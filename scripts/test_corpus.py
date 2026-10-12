#!/usr/bin/env python3
"""Build the compiler and run assertion-based Dream projects plus isolated Rust corpus tests."""
import argparse
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


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
    build = subprocess.run(["cargo", "build", "--workspace", "--quiet"], cwd=ROOT, env=env)
    if build.returncode:
        return build.returncode
    return subprocess.run([
        "cargo", "test", "-p", "dream", "--test", "integration", "corpus::run::full_corpus",
        "--", "--ignored", "--exact", "--nocapture",
    ], cwd=ROOT, env=env).returncode


if __name__ == "__main__":
    sys.exit(main())

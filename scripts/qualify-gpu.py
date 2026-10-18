#!/usr/bin/env python3
"""Run the existing GPU assertions on one explicitly requested native backend."""

import argparse
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
BACKENDS = {"vulkan": "Vulkan", "dx12": "Dx12", "cuda": "Cuda"}
CASES = {
    "gpu_compute_saxpy": "saxpy and switch match the CPU",
    "gpu_bindless_sampling": "bindless textures match both materials",
    "gpu_ray_query": "ray queries match distances and instance IDs",
    "gpu_ray_tracing_pipeline": "ray-tracing pipeline matches hit and miss references",
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--backend", choices=BACKENDS, required=True)
    parser.add_argument("--compiler", type=Path, default=ROOT / "target/debug" / ("dream.exe" if os.name == "nt" else "dream"))
    parser.add_argument("--output", type=Path, default=ROOT / "target/gpu-qualification")
    args = parser.parse_args()
    backend = BACKENDS[args.backend]
    project = args.output.resolve() / args.backend
    project.mkdir(parents=True, exist_ok=True)
    (project / "dream.toml").write_text(
        f'[package]\nname = "gpu_qualification_{args.backend}"\nversion = "0.1.0"\ntype = "test"\n',
        encoding="utf-8",
    )
    cases = dict(list(CASES.items())[:1]) if args.backend == "cuda" else CASES
    for name in cases:
        source = (ROOT / "tests/projects/gpu_runtime" / f"{name}.dream").read_text(encoding="utf-8")
        source = re.sub(r"@gpu_targets\([^)]*\)", f"@gpu_targets(GpuTarget.{backend})", source)
        (project / f"{name}.dream").write_text(source, encoding="utf-8")
    env = dict(os.environ, DREAM_GPU_STRICT_AOT="1")
    result = subprocess.run(
        [str(args.compiler.resolve()), "test", str(project), "--native"],
        cwd=ROOT, env=env, capture_output=True, text=True,
    )
    output = result.stdout + result.stderr
    (project / "qualification.log").write_text(output, encoding="utf-8")
    print(output, end="")
    if result.returncode:
        return result.returncode
    missing = [f"{backend} {message}" for message in cases.values() if f"{backend} {message}" not in output]
    if missing:
        print("GPU qualification did not execute: " + ", ".join(missing), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())

"""Build real Dream static libraries and their Rust core for mobile SDK checks."""

import json
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]
SAMPLE = ROOT / "sample/mobile"
OUTPUT = ROOT / "target/mobile-validation"
PACKAGE = "org.example.dreamvalidation"
PASS = "DREAM_MOBILE_PASS answer=42"


def run(*args, env=None, cwd=ROOT, timeout=1800):
    command = [str(arg) for arg in args]
    print("+ " + " ".join(command), flush=True)
    subprocess.run(command, cwd=cwd, env=env, check=True, timeout=timeout)


def capture(*args, env=None, timeout=60):
    return subprocess.check_output(
        [str(arg) for arg in args], env=env, text=True, timeout=timeout
    ).strip()


def prepare(platform):
    output = OUTPUT / platform
    output.mkdir(parents=True, exist_ok=True)
    project = output / "library"
    shutil.copytree(SAMPLE / "library", project, dirs_exist_ok=True)
    suffix = ".exe" if os.name == "nt" else ""
    dream = ROOT / f"target/debug/dream{suffix}"
    dreamer = ROOT / f"target/debug/dreamer{suffix}"
    if not dream.is_file() or not dreamer.is_file():
        raise RuntimeError("Build dream and dreamer before running mobile validation")
    return output, project, dream, dreamer


def build_slice(output, project, dream, rust_target, dream_target, env, extension):
    rust_env = env.copy()
    # DREAM_SYSROOT belongs to the guest compiler, not Cargo's host build scripts.
    rust_env.pop("DREAM_SYSROOT", None)
    run("rustup", "target", "add", rust_target)
    run(
        "cargo", "rustc", "--locked", "-p", "dream-host-core", "--lib",
        "--release", "--target", rust_target, "--target-dir", output / "host",
        "--crate-type", "staticlib,cdylib", env=rust_env,
    )
    host = output / "host" / rust_target / "release"
    target_lib = output / "targets" / dream_target / "lib"
    target_lib.mkdir(parents=True, exist_ok=True)
    shutil.copy2(host / f"libdream_host_core.{extension}", target_lib)
    guest_env = env.copy()
    guest_env["DREAM_TARGETS"] = str(output / "targets")
    guest_env["DREAM_BIN"] = str(dream)
    directory = output / "slices" / dream_target
    directory.mkdir(parents=True, exist_ok=True)
    archive = directory / "libmobile_demo.a"
    run(
        dream, "--target", dream_target, project / "src/lib.dream",
        "-o", archive, env=guest_env,
    )
    run(dream, "toolchain-doctor", "--target", dream_target, "--json", env=guest_env)
    abi = json.loads(archive.with_suffix(".abi.json").read_text())
    if abi["exports"] != ["answer"]:
        raise RuntimeError("Sample library must expose its typed @export function only")
    return archive, host / "libdream_host_core.a", guest_env


def result(platform, details):
    report = {"platform": platform, "passed": True, "expected": PASS, **details}
    (OUTPUT / platform / "result.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report), flush=True)

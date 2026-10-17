#!/usr/bin/env python3
"""Milestone 0 strict-AOT probe driver (not part of the cargo workspace build).

    python3 build.py [backend ...]   fetch pins, build, compile shaders, run the probe
    python3 build.py pin             download every pinned archive and print BLAKE3 hashes

Backends default to everything slang-rhi can try on the host (metal vulkan wgpu on macOS).
Downloads and builds go to $DREAM_GPU_CACHE, else ${CARGO_TARGET_DIR:-<repo>/target}/gpu-cache.
"""

from __future__ import annotations

import hashlib
import json
import os
import platform
import shutil
import struct
import subprocess
import sys
import tarfile
import tomllib
import urllib.request
import venv
import zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
PATCHES = HERE.parent / "patches"
PINS = tomllib.loads((HERE.parent / "pins.toml").read_text())


def cache_dir() -> Path:
    if os.environ.get("DREAM_GPU_CACHE"):
        return Path(os.environ["DREAM_GPU_CACHE"])
    target = Path(os.environ.get("CARGO_TARGET_DIR", REPO / "target"))
    return target / "gpu-cache"


CACHE = cache_dir()


def ensure_blake3():
    """Re-executes under a cache venv that has the `blake3` package when it is missing."""
    try:
        import blake3  # noqa: F401
    except ImportError:
        env_dir = CACHE / "venv"
        python = env_dir / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
        if Path(sys.prefix).resolve() == env_dir.resolve():
            sys.exit("blake3 is not installable into the cache venv")
        if not python.exists():
            venv.create(env_dir, with_pip=True)
            subprocess.check_call([str(python), "-m", "pip", "install", "-q", "blake3"])
        os.execv(str(python), [str(python), *sys.argv])


def host_platform() -> str:
    system = {"Darwin": "macos", "Linux": "linux", "Windows": "windows"}[platform.system()]
    machine = platform.machine().lower()
    arch = "aarch64" if machine in ("arm64", "aarch64") else "x86_64"
    return f"{system}-{arch}"


def blake3_file(path: Path) -> str:
    import blake3

    return blake3.blake3(path.read_bytes()).hexdigest()


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def fetch(url: str, dest: Path):
    # Homebrew bottles live on ghcr.io, which wants an (anonymous) bearer token.
    headers = {"Authorization": "Bearer QQ=="} if url.startswith("https://ghcr.io/") else {}
    if shutil.which("curl"):
        cmd = ["curl", "-fsSL", "--retry", "3", "-o", str(dest), url]
        for key, value in headers.items():
            cmd[1:1] = ["-H", f"{key}: {value}"]
        subprocess.check_call(cmd)
        return
    request = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(request, timeout=60) as response, open(dest, "wb") as out:
        shutil.copyfileobj(response, out)


def download(url: str, sha256: str, blake3: str | None = None, name: str | None = None) -> Path:
    dest = CACHE / "downloads" / (name or url.rsplit("/", 1)[-1])
    if not dest.exists() or sha256_file(dest) != sha256:
        dest.parent.mkdir(parents=True, exist_ok=True)
        print(f"download {url}")
        tmp = dest.with_suffix(dest.suffix + ".part")
        fetch(url, tmp)
        tmp.replace(dest)
    if sha256_file(dest) != sha256:
        sys.exit(f"{dest.name}: SHA-256 mismatch")
    if blake3 and blake3_file(dest) != blake3:
        sys.exit(f"{dest.name}: BLAKE3 mismatch")
    return dest


def extract(archive: Path, into: Path) -> Path:
    if into.exists():
        return into
    tmp = into.with_name(into.name + ".tmp")
    shutil.rmtree(tmp, ignore_errors=True)
    tmp.mkdir(parents=True)
    if archive.name.endswith(".zip"):
        with zipfile.ZipFile(archive) as z:
            z.extractall(tmp)
    else:
        with tarfile.open(archive) as t:
            t.extractall(tmp, filter="tar")
    tmp.rename(into)
    return into


def single_child(path: Path) -> Path:
    children = [c for c in path.iterdir() if not c.name.startswith(".")]
    return children[0] if len(children) == 1 and children[0].is_dir() else path


def pinned(name: str, plat: str | None = None) -> dict:
    entry = PINS[name]
    return entry["archives"][plat] if plat else entry


# ---------------------------------------------------------------------------------------------
# Inputs


def fetch_slang(plat: str) -> Path:
    pin = pinned("slang", plat)
    archive = download(pin["url"], pin["sha256"], pin.get("blake3"))
    return extract(archive, CACHE / f"slang-{PINS['slang']['version']}-{plat}")


def fetch_slang_rhi() -> Path:
    pin = PINS["slang-rhi"]
    archive = download(pin["url"], pin["sha256"], pin.get("blake3"))
    patches = [PATCHES / p for p in pin["patches"]]
    import blake3

    patch_id = blake3.blake3(b"".join(p.read_bytes() for p in patches)).hexdigest()[:12]
    dest = CACHE / f"slang-rhi-{pin['commit'][:12]}-{patch_id}"
    if dest.exists():
        return dest
    staging = extract(archive, CACHE / f"slang-rhi-{pin['commit'][:12]}-pristine")
    tmp = dest.with_name(dest.name + ".tmp")
    shutil.rmtree(tmp, ignore_errors=True)
    shutil.copytree(single_child(staging), tmp, symlinks=True)
    # Without a ceiling git would find the Dream checkout above the cache and apply relative
    # to it; with one, `git apply` works on the plain extracted tree.
    env = dict(os.environ, GIT_CEILING_DIRECTORIES=str(tmp.parent))
    for patch in patches:
        print(f"apply {patch.name}")
        subprocess.check_call(["git", "apply", "-p1", str(patch)], cwd=tmp, env=env)
    tmp.rename(dest)
    return dest


def fetch_url_input(name: str, plat: str | None = None) -> Path:
    pin = pinned(name, plat)
    return download(pin["url"], pin["sha256"], pin.get("blake3"))


def fetch_vulkan_on_macos() -> dict[str, str]:
    """Khronos loader + MoltenVK ICD, so the probe reaches slang-rhi's Vulkan backend."""
    if not host_platform().startswith("macos"):
        return {}
    pin = PINS["vulkan-loader"]
    dest = download(pin["url"], pin["sha256"], pin.get("blake3"),
                    name=f"vulkan-loader-{pin['version']}.bottle.tar.gz")
    loader = extract(dest, CACHE / f"vulkan-loader-{pin['version']}")
    loader_lib = next(loader.rglob("libvulkan.1.dylib")).parent
    mvk = extract(fetch_url_input("moltenvk"), CACHE / f"moltenvk-{PINS['moltenvk']['version']}")
    icd = next(mvk.rglob("dynamic/dylib/macOS/MoltenVK_icd.json"))
    return {"DYLD_LIBRARY_PATH": str(loader_lib), "VK_DRIVER_FILES": str(icd)}


# ---------------------------------------------------------------------------------------------
# Shader compilation


COMMON_ARGS = ["-matrix-layout-row-major"]
TARGETS = {
    # slang-rhi's Vulkan backend always names the SPIR-V entry point "main".
    "spirv": ["-target", "spirv"],
    "metal": ["-target", "metal"],
    "metallib": ["-target", "metallib"],
    "dxil": ["-target", "dxil", "-profile", "sm_6_5"],
    "dxbc": ["-target", "dxbc", "-profile", "sm_5_0"],
    "ptx": ["-target", "ptx"],
    "wgsl": ["-target", "wgsl"],
}
SHADERS = {
    "saxpy": {"entries": [("saxpy", "compute")], "capabilities": []},
}


def exe(path: Path) -> Path:
    return path.with_suffix(".exe") if os.name == "nt" else path


def first_error(text: str) -> str:
    for line in text.splitlines():
        if "error" in line:
            return line.strip()
    return text.strip().splitlines()[0] if text.strip() else ""


def compile_target(slang: Path, recorder: Path, shader: str, target: str, out: Path) -> dict:
    """slangc output + reflection JSON, plus the recorded layout from the same options."""
    spec = SHADERS[shader]
    source = HERE / "shaders" / f"{shader}.slang"
    options = COMMON_ARGS + TARGETS[target]
    result: dict = {"target": target, "options": options}
    entries = []
    for name, stage in spec["entries"]:
        code = out / f"{shader}.{name}.{target}"
        reflection = out / f"{shader}.{name}.{target}.json"
        cmd = [str(exe(slang / "bin" / "slangc")), str(source), "-entry", name, "-stage", stage,
               *options, "-o", str(code), "-reflection-json", str(reflection)]
        proc = subprocess.run(cmd, capture_output=True, text=True)
        entries.append({"name": name, "stage": stage, "code": code, "reflection": reflection,
                        "ok": proc.returncode == 0 and code.exists(),
                        "diagnostic": first_error(proc.stderr)})
    result["slangc"] = all(e["ok"] for e in entries)
    result["entries"] = entries

    layout = out / f"{shader}.{target}.dslr"
    cmd = [str(recorder), str(source)]
    for name, _ in spec["entries"]:
        cmd += ["-entry", name]
    cmd += [*options, "-o", str(layout)]
    proc = subprocess.run(cmd, capture_output=True, text=True)
    result["layout"] = layout if proc.returncode == 0 and layout.exists() else None
    result["recorder"] = first_error(proc.stderr)
    return result


# ---------------------------------------------------------------------------------------------
# Bundle (format proposal v0; see the Milestone 0 report)


def write_bundle(path: Path, shader: str, compiled: list[dict]) -> dict:
    import blake3

    # The native bundle carries one code set per backend; MSL source stands in for a
    # metallib when the build host has no Apple `metal` tool.
    chosen: dict[str, dict] = {}
    for c in compiled:
        if not (c["slangc"] and c["layout"]):
            continue
        backend_target = "metal" if c["target"] == "metallib" else c["target"]
        if backend_target not in chosen or c["target"] == "metallib":
            chosen[backend_target] = c

    data = bytearray()
    view: dict = {"format": "DGPB", "version": 0, "shader": shader, "targets": []}

    def add_blob(raw: bytes) -> tuple[int, int, bytes]:
        offset = len(data)
        data.extend(raw)
        return offset, len(raw), blake3.blake3(raw).digest()

    def s(text: str) -> bytes:
        raw = text.encode()
        return struct.pack("<I", len(raw)) + raw

    targets = bytearray()
    for name in sorted(chosen):
        c = chosen[name]
        layout = c["layout"].read_bytes()
        off, size, digest = add_blob(layout)
        targets += s(name) + struct.pack("<II", off, size) + digest
        tview = {"target": name, "code_format": c["target"],
                 "slangc_options": c["options"],
                 "layout": {"bytes": size, "blake3": digest.hex()}, "entry_points": [],
                 "capabilities": SHADERS[shader]["capabilities"]}
        targets += struct.pack("<I", len(c["entries"]))
        for e in c["entries"]:
            code = e["code"].read_bytes()
            off, size, digest = add_blob(code)
            targets += s(e["name"]) + s(e["stage"]) + struct.pack("<II", off, size) + digest
            reflection = json.loads(e["reflection"].read_text())
            tview["entry_points"].append({
                "name": e["name"], "stage": e["stage"], "bytes": size, "blake3": digest.hex(),
                "thread_group_size": next(
                    (ep.get("threadGroupSize") for ep in reflection.get("entryPoints", [])
                     if ep.get("name") == e["name"]), None),
                "parameters": [
                    {"name": p["name"], "binding": p.get("binding"),
                     "kind": p.get("type", {}).get("kind")}
                    for p in reflection.get("parameters", [])],
            })
        caps = SHADERS[shader]["capabilities"]
        targets += struct.pack("<I", len(caps)) + b"".join(s(cap) for cap in caps)
        view["targets"].append(tview)

    header_size = 16 + len(targets)
    header = b"DGPB" + struct.pack("<III", 0, header_size, len(chosen)) + targets
    path.write_bytes(header + data)
    view["bytes"] = path.stat().st_size
    view["blake3"] = blake3_file(path)
    path.with_suffix(".bundle.json").write_text(json.dumps(view, indent=2) + "\n")
    return view


# ---------------------------------------------------------------------------------------------
# Cooperative-vector probe


def coopvec_matrix(slang: Path, out: Path) -> dict:
    source = HERE / "shaders" / "coopvec.slang"
    findings = {}
    cases = {
        "spirv": ["-target", "spirv-asm"],
        "hlsl-sm_6_9 (dxil path)": ["-target", "hlsl", "-profile", "sm_6_9"],
        "cuda": ["-target", "cuda"],
        "cuda+optix_coopvec": ["-target", "cuda", "-capability", "optix_coopvec"],
        "metal": ["-target", "metal"],
        "wgsl": ["-target", "wgsl"],
    }
    markers = {
        "OpCooperativeVectorMatrixMulAddNV": "native SPV_NV_cooperative_vector",
        "__builtin_MatVecMulAdd": "native SM 6.9 linalg",
        "slangOptixCoopVecMatMul": "native OptiX cooperative vectors",
        "FixedArray<": "software emulation (per-element loops)",
    }
    for name, args in cases.items():
        dest = out / f"coopvec.{name.split()[0]}.out"
        proc = subprocess.run(
            [str(exe(slang / "bin" / "slangc")), str(source), "-entry", "coopvec", "-stage", "compute",
             *args, "-o", str(dest)],
            capture_output=True, text=True)
        if proc.returncode != 0 or not dest.exists():
            findings[name] = {"compiles": False, "diagnostic": first_error(proc.stderr)}
            continue
        text = dest.read_text(errors="replace")
        found = [label for marker, label in markers.items() if marker in text]
        findings[name] = {"compiles": True, "lowering": found or ["unclassified"]}
    return findings


# ---------------------------------------------------------------------------------------------


def configure_and_build(slang: Path, rhi: Path, plat: str) -> Path:
    build = CACHE / "build-probe"
    dawn = fetch_url_input("dawn", plat)
    vulkan = fetch_url_input("vulkan-headers")
    defines = {
        "CMAKE_BUILD_TYPE": "Release",
        "DREAM_SLANG_DIR": slang,
        "DREAM_SLANG_RHI_DIR": rhi,
        "FETCHCONTENT_BASE_DIR": CACHE / "fetchcontent",
        "SLANG_RHI_DAWN_URL": dawn.as_uri(),
        "SLANG_RHI_DAWN_URL_HASH": f"SHA256={PINS['dawn']['archives'][plat]['sha256']}",
        "SLANG_RHI_VULKAN_HEADERS_URL": vulkan.as_uri(),
        "SLANG_RHI_VULKAN_HEADERS_URL_HASH": f"SHA256={PINS['vulkan-headers']['sha256']}",
    }
    if plat.startswith("macos"):
        metal = fetch_url_input("metal-cpp")
        defines["SLANG_RHI_METAL_CPP_URL"] = metal.as_uri()
        defines["SLANG_RHI_METAL_CPP_URL_HASH"] = f"SHA256={PINS['metal-cpp']['sha256']}"
    generator = ["-G", "Ninja"] if shutil.which("ninja") else []
    subprocess.check_call(["cmake", "-S", str(HERE), "-B", str(build), *generator,
                           *[f"-D{k}={v}" for k, v in defines.items()]])
    subprocess.check_call(["cmake", "--build", str(build), "--target", "aot_probe",
                           "dream-slang-layout"])
    # slang-rhi loads Dawn from the directory of the binary that links it.
    dawn_lib = next((p for p in (CACHE / "fetchcontent").rglob("*dawn*")
                     if p.name in ("libdawn.dylib", "libdawn.so", "dawn.dll")), None)
    if dawn_lib:
        shutil.copy2(dawn_lib, build / dawn_lib.name)
    return build


def run_probe(build: Path, bundle: Path, backends: list[str], slang: Path) -> dict:
    env = dict(os.environ, **fetch_vulkan_on_macos())
    proc = subprocess.run([str(exe(build / "aot_probe")), str(bundle), *backends],
                          capture_output=True, text=True, env=env)
    print(proc.stdout, end="")
    print(proc.stderr, end="", file=sys.stderr)
    results, features, loader = {}, {}, None
    for line in proc.stdout.splitlines():
        parts = line.split(" ", 3)
        if parts[0] == "RESULT":
            results[parts[1]] = {"status": parts[2].rstrip(":"), "detail": parts[3] if len(parts) > 3 else ""}
        elif parts[0] == "FEATURE":
            features[parts[1]] = line.split(" ", 2)[2]
        elif parts[0] == "LOADER" and parts[1].rstrip(":") in ("pass", "fail"):
            loader = line
    compiler = next((slang / "lib").glob("libslang-compiler.*.dylib"), None) or \
        next((slang / "lib").glob("libslang-compiler.so*"), None)
    self_test = None
    if compiler:
        st = subprocess.run([str(exe(build / "aot_probe")), "--loader-self-test", str(compiler)],
                            capture_output=True, text=True)
        self_test = {"exit": st.returncode, "output": st.stdout.strip()}
    return {"exit": proc.returncode, "backends": results, "features": features,
            "loader": loader, "loader_self_test": self_test}


def default_backends(plat: str) -> list[str]:
    if plat.startswith("macos"):
        return ["metal", "vulkan", "wgpu"]
    if plat.startswith("windows"):
        return ["d3d12", "d3d11", "vulkan", "cuda", "wgpu"]
    return ["vulkan", "cuda", "wgpu"]


def main_probe(backends: list[str]):
    plat = host_platform()
    slang = single_child(fetch_slang(plat))
    rhi = fetch_slang_rhi()
    build = configure_and_build(slang, rhi, plat)
    recorder = exe(build / "dream-slang-layout")

    out = CACHE / "probe-out"
    shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True)
    compiled = [compile_target(slang, recorder, "saxpy", t, out) for t in TARGETS]
    bundle_view = write_bundle(out / "saxpy.bundle", "saxpy", compiled)
    probe = run_probe(build, out / "saxpy.bundle", backends or default_backends(plat), slang)

    report = {
        "host": plat,
        "compile": [{"target": c["target"], "slangc": c["slangc"],
                     "diagnostic": next((e["diagnostic"] for e in c["entries"] if not e["ok"]), ""),
                     "layout_recorded": bool(c["layout"]), "recorder": c["recorder"]}
                    for c in compiled],
        "bundle": bundle_view,
        "probe": probe,
        "coopvec": coopvec_matrix(slang, out),
        "pins": {
            "slangc_blake3": blake3_file(exe(slang / "bin" / "slangc")),
        },
    }
    (CACHE / "report.json").write_text(json.dumps(report, indent=2, default=str) + "\n")
    print(json.dumps({k: report[k] for k in ("compile", "coopvec")}, indent=2, default=str))
    print(f"report: {CACHE / 'report.json'}")
    sys.exit(probe["exit"])


def main_pin():
    """Prints a BLAKE3 for every pinned archive, downloading each one."""
    rows = []
    for name, entry in PINS.items():
        if "archives" in entry:
            for plat, pin in entry["archives"].items():
                path = download(pin["url"], pin["sha256"])
                rows.append((f"{name}.archives.{plat}", blake3_file(path)))
        elif "url" in entry:
            bottle = f"{name}-{entry['version']}.bottle.tar.gz" if "ghcr.io" in entry["url"] else None
            path = download(entry["url"], entry["sha256"], name=bottle)
            rows.append((name, blake3_file(path)))
    for key, digest in rows:
        print(f'[{key}] blake3 = "{digest}"')


if __name__ == "__main__":
    ensure_blake3()
    if sys.argv[1:2] == ["pin"]:
        main_pin()
    else:
        main_probe(sys.argv[1:])

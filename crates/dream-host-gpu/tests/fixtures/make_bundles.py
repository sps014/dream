#!/usr/bin/env python3
"""Writes the DGPB v0 test bundles in this directory (docs/internals/18-gpu-shader-bundle.md).

    python3 make_bundles.py [--check] [<slang toolchain root>]

The root defaults to $DREAM_SLANG, else ~/.dream/toolchains/slang-<version>; it must contain
bin/slangc and bin/dream-slang-layout (`dreamer toolchain install slang`). This stands in for the
system.gpu.shaders generator in host tests: same tools, same options, same binary format. Targets
whose downstream compiler is missing on this machine (dxil, dxbc, ptx off Windows/CUDA hosts)
are preserved from the committed bundle, skipped and listed.
"""

from __future__ import annotations

import hashlib
import json
import os
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
SHADERS = HERE.parent / "shaders"
VERSION = "2026.20"
COMMON = ["-matrix-layout-row-major", "-line-directive-mode", "none"]
# target name -> (code format, extra slangc options)
TARGETS = {
    "dxbc": ("dxbc", ["-profile", "sm_5_0"]),
    "dxil": ("dxil", ["-profile", "sm_6_5"]),
    "metal": ("msl", []),
    "ptx": ("ptx", []),
    "spirv": ("spirv", []),
    "wgsl": ("wgsl", []),
}
# GpuArgKind numbering.
BUFFER, TEXTURE, SAMPLER, STORAGE_TEXTURE, ACCELERATION, VALUE, MLP = 1, 2, 3, 4, 5, 6, 7
READ_ONLY, WRITTEN = 1, 2

# module -> entries (name, stage, parameters). A parameter is (slang path, kind, flags[, bundle
# name]); a path "block.field" is a value packed into a uniform block. The bundle name defaults to
# the Slang name.
MODULES = {
    "saxpy": [
        ("saxpy", "compute", [
            ("x", BUFFER, READ_ONLY),
            ("y", BUFFER, READ_ONLY),
            ("result", BUFFER, WRITTEN),
            ("params.a", VALUE, 0),
            ("params.n", VALUE, 0),
        ]),
        ("scale", "compute", [
            ("data", BUFFER, WRITTEN),
            ("factor", VALUE, 0),
            ("count", VALUE, 0),
        ]),
    ],
    # Bundle names are the Dream parameter names, as the generator writes them; the host must
    # bind these by binding number alone.
    "generated": [
        ("fill", "compute", [
            ("fill_out", BUFFER, WRITTEN, "out"),
            ("fill_params.value", VALUE, 0, "value"),
            ("fill_params.count", VALUE, 0, "count"),
        ]),
        ("offset", "compute", [
            ("offset_data", BUFFER, WRITTEN, "data"),
            ("offset_params.amount", VALUE, 0, "amount"),
        ]),
    ],
    "quad": [
        ("quad_vertex", "vertex", []),
        ("quad_fragment", "fragment", [
            ("quad_fragment_albedo", TEXTURE, READ_ONLY, "albedo"),
            ("quad_fragment_nearest", SAMPLER, 0, "nearest"),
        ]),
    ],
    "triangle": [
        ("triangle_vertex", "vertex", []),
        ("triangle_fragment", "fragment", [("tint", VALUE, 0)]),
    ],
}


def tool_root() -> Path:
    roots = [a for a in sys.argv[1:] if not a.startswith("--")]
    if roots:
        return Path(roots[0])
    if os.environ.get("DREAM_SLANG"):
        return Path(os.environ["DREAM_SLANG"])
    return Path.home() / ".dream" / "toolchains" / f"slang-{VERSION}"


def run(cmd: list[str]) -> subprocess.CompletedProcess:
    return subprocess.run([str(c) for c in cmd], capture_output=True, text=True)


def binding_of(node: dict) -> dict:
    binding = node.get("binding")
    if binding is None and node.get("bindings"):
        binding = next((b for b in node["bindings"] if b.get("kind") != "uniform"), node["bindings"][0])
    return binding or {}


def space_binding(binding: dict) -> tuple[int, int]:
    if binding.get("kind") == "uniform":
        return 0, binding.get("offset", 0)
    if binding.get("kind") in ("subElementRegisterSpace", "registerSpace"):
        return binding.get("index", 0), 0
    return binding.get("space", 0), binding.get("index", 0)


def find(params: list[dict], name: str) -> dict | None:
    return next((p for p in params if p.get("name") == name), None)


def param_row(reflection: dict, entry: dict, path: str, kind: int, flags: int, bundle_name: str = "") -> tuple:
    row = slang_row(reflection, entry, path, kind, flags)
    return (bundle_name or row[0], *row[1:])


def slang_row(reflection: dict, entry: dict, path: str, kind: int, flags: int) -> tuple:
    name = path.split(".")[-1]
    if "." in path:
        block_name, field = path.split(".")
        block = find(reflection["parameters"], block_name) or find(entry.get("parameters", []), block_name)
        space, binding = space_binding(binding_of(block))
        element = block["type"].get("elementType", {})
        member = find(element.get("fields", []), field)
        layout = member["binding"]
        return (name, kind, flags, space, binding, layout["offset"], layout["size"])
    node = find(entry.get("parameters", []), name) or find(reflection["parameters"], name)
    if node is None:
        sys.exit(f"{entry['name']}: no parameter {name} in the reflection")
    bound = binding_of(node)
    if bound.get("kind") == "uniform":
        if kind != VALUE:
            return (name, kind, flags, 0, bound["offset"], 0, 0)
        # An entry-point `uniform`: report the entry block it was packed into.
        space, binding = space_binding(binding_of(entry.get("scope", {})))
        return (name, kind, flags, space, binding, bound["offset"], bound["size"])
    space, binding = space_binding(bound)
    return (name, kind, flags, space, binding, 0, 0)


def compile_target(root: Path, module: str, target: str, work: Path) -> dict | None:
    code_format, extra = TARGETS[target]
    source = SHADERS / f"{module}.slang"
    options = [*COMMON, "-target", target, *extra, "-D", f"DREAM_TARGET_{target.upper()}"]
    entries = MODULES[module]
    code_args = []
    for name, _, _ in entries:
        code_args += ["-entry", name, "-o", work / f"{module}.{name}.{target}"]
    reflections = {}
    for name, stage, _ in entries:
        reflection_path = work / f"{module}.{name}.{target}.json"
        compiled = run([root / "bin" / "slangc", source, *options, "-entry", name,
                        "-stage", stage, "-o", work / f"{module}.{name}.{target}",
                        "-reflection-json", reflection_path])
        if compiled.returncode != 0:
            return None
        reflections[name] = json.loads(reflection_path.read_text())
    layout = work / f"{module}.{target}.layout"
    recorded = run([root / "bin" / "dream-slang-layout", source, *code_args, *options, "-o", layout])
    if recorded.returncode != 0:
        sys.exit(f"dream-slang-layout failed for {module}/{target}:\n{recorded.stderr}")
    out_entries = []
    for name, stage, params in entries:
        reflection = reflections[name]
        entry = find(reflection["entryPoints"], name)
        size = entry.get("threadGroupSize", [0, 0, 0]) if stage == "compute" else [0, 0, 0]
        rows = [param_row(reflection, entry, *p) for p in params]
        out_entries.append((name, stage, (work / f"{module}.{name}.{target}").read_bytes(), size, rows, []))
    # One layout recorded with every entry; the generator records one per pipeline group.
    return {"name": target, "code_format": code_format, "layouts": [layout.read_bytes()],
            "entries": out_entries}


def write_bundle(targets: list[dict]) -> bytes:
    data = bytearray()
    stored: dict[bytes, int] = {}

    def u32(v: int) -> bytes:
        return struct.pack("<I", v)

    def string(text: str) -> bytes:
        raw = text.encode()
        return u32(len(raw)) + raw

    def blob(raw: bytes) -> bytes:
        if raw not in stored:
            stored[raw] = len(data)
            data.extend(raw)
        return u32(stored[raw]) + u32(len(raw)) + hashlib.sha256(raw).digest()

    header = bytearray(u32(len(targets)))
    for t in sorted(targets, key=lambda t: t["name"]):
        header += string(t["name"]) + string(t["code_format"])
        header += u32(len(t["layouts"])) + b"".join(blob(layout) for layout in t["layouts"])
        header += u32(len(t["entries"]))
        for name, stage, code, size, params, capabilities in t["entries"]:
            header += string(name) + string(stage) + u32(0) + blob(code) + b"".join(u32(v) for v in size)
            header += u32(len(params))
            for pname, kind, flags, space, binding, offset, psize in params:
                header += string(pname) + b"".join(u32(v) for v in (kind, flags, space, binding, offset, psize))
            header += u32(len(capabilities)) + b"".join(string(c) for c in capabilities)
    return b"DGPB" + u32(0) + u32(12 + len(header)) + bytes(header) + bytes(data)


def read_bundle(bundle: bytes) -> list[dict]:
    if bundle[:8] != b"DGPB\0\0\0\0":
        raise ValueError("unsupported bundle header")
    data_start, count = struct.unpack_from("<II", bundle, 8)
    pos = 16

    def u32() -> int:
        nonlocal pos
        value = struct.unpack_from("<I", bundle, pos)[0]
        pos += 4
        return value

    def string() -> str:
        nonlocal pos
        n = u32()
        raw = bundle[pos:pos + n]
        pos += n
        if len(raw) != n or pos > data_start:
            raise ValueError("truncated bundle string")
        return raw.decode()

    def blob() -> bytes:
        nonlocal pos
        offset, size = u32(), u32()
        digest = bundle[pos:pos + 32]
        pos += 32
        raw = bundle[data_start + offset:data_start + offset + size]
        if pos > data_start or len(raw) != size or hashlib.sha256(raw).digest() != digest:
            raise ValueError("invalid bundle blob")
        return raw

    targets = []
    for _ in range(count):
        name, code_format = string(), string()
        layouts = [blob() for _ in range(u32())]
        entries = []
        for _ in range(u32()):
            entry, stage = string(), string()
            if u32() != 0:
                raise ValueError("fixture entries must use layout 0")
            code = blob()
            size = tuple(u32() for _ in range(3))
            params = [(string(), *(u32() for _ in range(6))) for _ in range(u32())]
            capabilities = [string() for _ in range(u32())]
            entries.append((entry, stage, code, size, params, capabilities))
        targets.append(dict(name=name, code_format=code_format, layouts=layouts, entries=entries))
    if pos != data_start:
        raise ValueError("unexpected bundle header fields")
    return targets


def blob_digests(bundle: bytes) -> dict[str, str]:
    """`target/layout<i>` and `target/<entry>` -> SHA-256 of each blob, to say what changed."""
    out = {}
    for target in read_bundle(bundle):
        prefix = target["name"]
        for i, layout in enumerate(target["layouts"]):
            out[f"{prefix}/layout{i}"] = hashlib.sha256(layout).hexdigest()
        for name, _, code, *_ in target["entries"]:
            out[f"{prefix}/{name}"] = hashlib.sha256(code).hexdigest()
    return out


def describe_difference(old: bytes, new: bytes) -> str:
    try:
        before, after = blob_digests(old), blob_digests(new)
    except (ValueError, struct.error, UnicodeDecodeError, IndexError):
        return "the committed bundle does not parse"
    changed = sorted(k for k in before.keys() | after.keys() if before.get(k) != after.get(k))
    return ", ".join(changed) if changed else "header fields only"


def main():
    check = "--check" in sys.argv[1:]
    root = tool_root()
    for tool in ("slangc", "dream-slang-layout"):
        if not (root / "bin" / tool).exists() and not (root / "bin" / f"{tool}.exe").exists():
            sys.exit(f"{root}/bin/{tool} is missing; run `dreamer toolchain install slang`")
    stale = []
    with tempfile.TemporaryDirectory() as tmp:
        for module in MODULES:
            built, skipped = [], []
            for target in TARGETS:
                compiled = compile_target(root, module, target, Path(tmp))
                (built if compiled else skipped).append(compiled or target)
            path = HERE / f"{module}.dgpb"
            if not built:
                sys.exit(f"{module}: no shader target compiled; refusing to write an empty bundle")
            # A Linux host cannot rebuild DXIL/PTX, but must not remove Windows-built targets.
            retained = [t for t in read_bundle(path.read_bytes()) if t["name"] in skipped] if path.exists() else []
            bundle = write_bundle(built + retained)
            if check:
                if not path.exists():
                    stale.append(f"{path.name} (missing)")
                elif path.read_bytes() != bundle:
                    stale.append(f"{path.name} ({describe_difference(path.read_bytes(), bundle)})")
            else:
                # Rust/editor builds may be reading the old fixture while it is regenerated.
                with tempfile.NamedTemporaryFile(dir=HERE, delete=False) as output:
                    staged = Path(output.name)
                    output.write(bundle)
                try:
                    staged.replace(path)
                finally:
                    staged.unlink(missing_ok=True)
            print(f"{module}.dgpb: {', '.join(t['name'] for t in built)}"
                  + (f" (skipped {', '.join(skipped)})" if skipped else ""))
    if stale:
        sys.exit(f"stale bundles: {', '.join(stale)}; run `python3 {Path(__file__).name}` and commit them")


if __name__ == "__main__":
    main()

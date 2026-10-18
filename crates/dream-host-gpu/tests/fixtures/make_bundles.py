#!/usr/bin/env python3
"""Writes the DGPB v0 test bundles in this directory (docs/internals/18-gpu-shader-bundle.md).

    python3 make_bundles.py [--check] [<slang toolchain root>]

The root defaults to $DREAM_SLANG, else ~/.dream/toolchains/slang-<version>; it must contain
bin/slangc and bin/dream-slang-layout (`dreamer toolchain install slang`). This stands in for the
system.gpu.shaders generator in host tests: same tools, same options, same binary format. Targets
whose downstream compiler is missing on this machine (dxil, dxbc, ptx off Windows/CUDA hosts)
are skipped and listed.
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
            ("data", BUFFER, READ_ONLY | WRITTEN),
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
            ("offset_data", BUFFER, READ_ONLY | WRITTEN, "data"),
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
        # An entry-point `uniform`: report the entry block it was packed into.
        space, binding = space_binding(binding_of(entry.get("scope", {})))
        return (name, kind, flags, space, binding, bound["offset"], bound["size"])
    space, binding = space_binding(bound)
    return (name, kind, flags, space, binding, 0, 0)


def compile_target(root: Path, module: str, target: str, work: Path) -> dict | None:
    code_format, extra = TARGETS[target]
    source = SHADERS / f"{module}.slang"
    options = [*COMMON, "-target", target, *extra]
    entries = MODULES[module]
    entry_args = [arg for name, _, _ in entries for arg in ("-entry", name)]
    reflection_path = work / f"{module}.{target}.json"
    combined = run([root / "bin" / "slangc", source, *entry_args, *options,
                    "-o", work / f"{module}.{target}.all", "-reflection-json", reflection_path])
    if combined.returncode != 0:
        return None
    layout = work / f"{module}.{target}.layout"
    code_args = []
    for name, _, _ in entries:
        code_args += ["-entry", name, "-o", work / f"{module}.{name}.{target}"]
    recorded = run([root / "bin" / "dream-slang-layout", source, *code_args, *options, "-o", layout])
    if recorded.returncode != 0:
        sys.exit(f"dream-slang-layout failed for {module}/{target}:\n{recorded.stderr}")
    reflection = json.loads(reflection_path.read_text())
    out_entries = []
    for name, stage, params in entries:
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
            bundle = write_bundle(built)
            if check:
                if not path.exists() or path.read_bytes() != bundle:
                    stale.append(path.name)
            else:
                path.write_bytes(bundle)
            print(f"{module}.dgpb: {', '.join(t['name'] for t in built)}"
                  + (f" (skipped {', '.join(skipped)})" if skipped else ""))
    if stale:
        sys.exit(f"stale bundles: {', '.join(stale)}; run `python3 {Path(__file__).name}` and commit them")


if __name__ == "__main__":
    main()

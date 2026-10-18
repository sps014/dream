"""Hold function placement constant across compiler revisions in native comparisons."""
import hashlib
from pathlib import Path
import platform
import shlex


def toolchain(llvm: Path, out: Path, functions: int, loops: int) -> Path:
    if platform.system() != "Linux":
        raise ValueError("controlled code alignment requires a native Linux measurement")
    for alignment in (functions, loops):
        if alignment and (alignment < 16 or alignment & (alignment - 1)):
            raise ValueError("code alignment must be a power of two of at least 16 bytes")
    llvm = llvm.resolve()
    if (llvm / "bin").is_dir():
        llvm /= "bin"
    if not (llvm / "llc").is_file():
        raise ValueError(f"missing LLVM code generator: {llvm / 'llc'}")
    identity = hashlib.sha256(str(llvm).encode()).hexdigest()[:12]
    root = out.resolve() / f"llvm-aligned-{functions}-{loops}-{identity}"
    binary = root / "bin"
    binary.mkdir(parents=True, exist_ok=True)
    for source in [*llvm.iterdir(), *llvm.parent.iterdir()]:
        if (source.parent == llvm and source.name == "llc") or source == llvm:
            continue
        destination = (binary if source.parent == llvm else root) / source.name
        if not destination.is_symlink():
            destination.symlink_to(source, target_is_directory=source.is_dir())
    wrapper = binary / "llc"
    if wrapper.is_symlink():
        raise ValueError(f"expected a benchmark wrapper, found a symlink: {wrapper}")
    flags = []
    if functions:
        flags.append(f"-align-all-functions={functions.bit_length() - 1}")
    if loops:
        flags.append(f"-align-loops={loops}")
    wrapper.write_text("#!/bin/sh\nexec " + shlex.quote(str(llvm / "llc"))
                       + " " + " ".join(flags) + ' "$@"\n')
    wrapper.chmod(0o755)
    return binary

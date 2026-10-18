"""Hold function placement constant across compiler revisions in native comparisons."""
import hashlib
from pathlib import Path
import platform
import shlex


def toolchain(llvm: Path, out: Path, alignment: int) -> Path:
    if platform.system() != "Linux":
        raise ValueError("controlled function alignment requires a native Linux measurement")
    if alignment < 16 or alignment & (alignment - 1):
        raise ValueError("function alignment must be a power of two of at least 16 bytes")
    llvm = llvm.resolve()
    if (llvm / "bin").is_dir():
        llvm /= "bin"
    if not (llvm / "llc").is_file():
        raise ValueError(f"missing LLVM code generator: {llvm / 'llc'}")
    identity = hashlib.sha256(str(llvm).encode()).hexdigest()[:12]
    root = out.resolve() / f"llvm-aligned-{alignment}-{identity}"
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
    wrapper.write_text(
        "#!/bin/sh\nexec " + shlex.quote(str(llvm / "llc"))
        + f' -align-all-functions={alignment.bit_length() - 1} "$@"\n'
    )
    wrapper.chmod(0o755)
    return binary

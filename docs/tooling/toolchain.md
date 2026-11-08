---
title: Toolchain
summary: Install and check the build tools Dream needs.
icon: material/hammer-screwdriver
---

# Install and check build tools

The installer sets up the tools Dream needs to build programs. If a build reports a missing tool, check the installation first:

```sh
dreamer toolchain list
dreamer toolchain doctor
```

`doctor` reports the tools and libraries Dream found, and does not change your installation. Add `--json` for a machine-readable report.

## Install a component

```sh
dreamer toolchain install binaryen
```

| Component | When you need it |
| --- | --- |
| `binaryen` | Optimizing WebAssembly output (`--release` / `-O`) |
| `slang` | Generators that compile shaders with `slangc`. On Windows the install also copies `dxcompiler.dll` and `dxil.dll` from the Windows SDK into the Slang `bin` directory, which DXIL targets need |

Native builds use the compiler and linker that ship with Dream, not a compiler from your system. Releases include those tools, and development builds install them with `scripts/fetch-dev-llvm.sh` (`scripts/fetch-dev-llvm.ps1` on Windows), which needs `cargo`.

The first optimized WebAssembly build runs `dreamer toolchain install binaryen` when that tool is missing. Set `DREAM_NO_AUTO_INSTALL=1` for an offline build, and install Binaryen ahead of time.

Tools live under `~/.dream/toolchains/<name>-<version>`. `dreamer toolchain install` with no component installs `binaryen` (`dreamer toolchain list` shows what is installed), while `dreamer install` installs project packages.

## Custom installations

`DREAM_WASM_OPT`, `DREAM_LLVM`, `DREAM_MOLD`, and `DREAM_SLANG` select those tools (the install folder or its `bin/`). Use the version Dream expects, because another installation may not support its build commands.

Check another platform with `dreamer toolchain doctor --target <target-triple>`. Remove a component with `dreamer toolchain uninstall <component>` when you no longer need it.

See [Troubleshooting](../learn/troubleshooting.md) for installation and command-discovery problems.

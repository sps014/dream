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

`doctor` reports the tools and libraries Dream found. It does not change your installation. Add `--json` if you need a machine-readable report.

## Install a component

```sh
dreamer toolchain install cc
dreamer toolchain install binaryen
```

| Component | When you need it |
| --- | --- |
| `cc` | Building desktop programs when no suitable system linker is installed |
| `binaryen` | Optimizing WebAssembly output (`--release` / `-O`) |
| `slang` | Generators that compile shaders with `slangc` |

Dream ships LLVM with releases; development builds use `scripts/fetch-dev-llvm.sh`. Binaryen is a prebuilt, pinned executable rather than a Cargo dependency. The first optimized WASM build automatically runs `dreamer toolchain install binaryen` if needed. Set `DREAM_NO_AUTO_INSTALL=1` for offline builds and install it ahead of time.

Tools are stored under `~/.dream/toolchains/<name>-<version>`. `dreamer toolchain install` without a component installs `cc` and `binaryen`; `dreamer toolchain list` shows every component and whether it is installed. `dreamer install` installs project packages instead.

## Custom installations

`DREAM_WASM_OPT` selects the pinned `wasm-opt` executable and `DREAM_ZIG` the Zig executable. `DREAM_LLVM` selects your LLVM tools and `DREAM_SLANG` your Slang installation (each the install folder or its `bin/`). `DREAM_CC` or `CC` selects your native linker driver. Use the version Dream expects; a different installation may not support its build commands.

Check another platform with `dreamer toolchain doctor --target <target-triple>`. Remove a component with `dreamer toolchain uninstall <component>` when you no longer need it.

See [Troubleshooting](../learn/troubleshooting.md) for installation and command-discovery problems.

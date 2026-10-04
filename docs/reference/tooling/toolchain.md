# Install and check build tools

The installer sets up the tools Dream needs to build programs. If a build reports a missing tool, check the installation first:

```sh
dreamer toolchain list
dreamer toolchain doctor
```

`doctor` reports the tools and libraries Dream found. It does not change your installation. Add `--json` if you need a machine-readable report.

## Install a component

```sh
dreamer toolchain install llvm
dreamer toolchain install cc
dreamer toolchain install wasi-sdk
```

| Component | When you need it |
| --- | --- |
| `llvm` | Building any Dream program |
| `cc` | Building desktop programs when no suitable system linker is installed |
| `wasi-sdk` | Building WebAssembly output |

Tools are stored under `~/.dream/toolchains/`. `dreamer toolchain install` without a component installs every component available for your computer. `dreamer install` installs project packages instead.

## Custom installations

`DREAM_LLVM` selects your LLVM tools. `DREAM_CC` or `CC` selects your native linker driver. Use the version Dream expects; a different installation may not support its build commands.

Check another platform with `dreamer toolchain doctor --target <target-triple>`. Remove a component with `dreamer toolchain uninstall <component>` when you no longer need it.

See [Troubleshooting](../../learn/troubleshooting.md) for installation and command-discovery problems.

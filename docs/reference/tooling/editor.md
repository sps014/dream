# Set up your editor

Dream's editor tools help you find mistakes while typing, complete names, format code, and navigate to declarations.

## Get started

Install Dream first and check `dream --help` and `dreamer --help` in a new terminal. Install the Dream extension in your editor, then open the project folder containing `dream.toml`.

If you are using the extension from this repository, build and package it using the instructions in the [tooling README](https://github.com/sps014/dream/blob/main/tooling/README.md).

## When the editor cannot find Dream

Restart the editor after installation. Set `dream.home` and `dreamer.home` to the folder containing their executables if automatic discovery does not work. The extension starts `dream-lsp` for language support.

## Project support

Install dependencies with `dreamer install` so imported packages are available. Open a workspace's root folder when working on several packages. See [Workspaces](workspaces.md).

The settings below come from the extension's configuration. Change them in your editor's settings panel.

## Settings

| Setting | Default | Purpose |
| --- | --- | --- |
| `dream.buildMode` | `"debug"` | Compile/run build mode. Release enables release memory settings + WebAssembly optimization (default -Os unless optimize level overrides). |
| `dream.optimizeLevel` | `"default"` | WebAssembly optimization optimization level passed as -O<level>. Default leaves the level to the build mode. |
| `dream.runtimeTarget` | `"native"` | Execution/runtime target for Run and compile commands. |
| `dream.home` | `""` | Directory containing the `dream` and `dream-lsp` binaries. Overrides DREAM_HOME when set. Required for Run/Debug/LSP unless those binaries are on PATH (e.g. after `source ./use-toolchain.sh`). |
| `dreamer.home` | `""` | Directory containing the `dreamer` package-manager binary. Overrides DREAMER_HOME when set. |

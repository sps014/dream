---
title: CLI packaging
summary: "Build a relocatable executable for Windows, macOS, or Linux."
icon: material/laptop
---

# Package a CLI program

`dreamer pack` builds a runnable package and writes a relocatable executable to
`target/pack/<os>-<arch>/<name>-<os>-<arch>` (`.exe` on Windows).
Only host libraries selected by live calls are copied beside the executable.
The capability libraries are core, Unicode, crypto, process, timezone, net, CLI, data, and I/O;
programs without host calls require no host libraries.

| `--target` | Platform |
|---|---|
| `linux-x64` | Linux, x86-64 |
| `linux-arm64` | Linux, ARM64 |
| `macos-x64` | macOS, Intel |
| `macos-arm64` | macOS, Apple silicon |
| `windows-x64` | Windows, x86-64 |
| `windows-arm64` | Windows, ARM64 |
| `all` | Every target above; cannot be combined with others |

Without `--target`, `pack` builds for the current machine. Repeat `--target` to build several.

| Option | Effect |
|---|---|
| `--release` | Release build; this is already the default |
| `-O <level>` / `--optimize <level>` | Optimization level (`0`–`4`, `s`, `z`) instead of the default `3` |
| `-p <name>` / `--package <name>` | Workspace member to package |

Each output folder contains:

| File | What it is |
|---|---|
| `<name>-<os>-<arch>` (`.exe` on Windows) | The program; embeds `[package].icon` when set |
| `libdream_host_<capability>.dylib` / `.so`, or `dream_host_<capability>.dll` | Host capability libraries the program calls |
| Your `[package].assets` | Files the program reads at run time |

```sh
dreamer pack
dreamer pack -O2
dreamer pack --target linux-arm64
dreamer pack --target all
```

Keep the executable and its adjacent libraries together when sharing or moving
the output folder. Files listed in `[package].assets` are copied beside the
executable too. Target machines need a compatible OS but do not need the
Dream compiler or toolchain. Packaging requires a bin package. Browser and Node
output uses WebAssembly; mobile library packaging has a separate workflow.

---
title: Options and troubleshooting
summary: Packaging options and fixes for common packaging problems.
icon: material/wrench-outline
---

# Packaging options and troubleshooting

Start with a working release build before changing packaging options.

| Option | Purpose |
| --- | --- |
| `--target windows-x64` | Package for a specific native platform and architecture |
| `--target all` | Build all supported native targets; each needs its own build requirements |
| `-O2` | Choose optimization level 2 instead of the default release level |
| `-p name` | Choose the package in a workspace |
| `[package].icon` | Set the app icon in `dream.toml` |

## A target cannot build

Run `dreamer toolchain doctor --target <target-triple>` to list missing build tools and libraries without installing them. Install what it lists from the [toolchain guide](../tooling/toolchain.md); the target platform may still need its SDK.

## A packaged app cannot start

Copy the entire target pack folder, including its libraries. The receiving machine also needs the platform's system libraries, and signing and notarization are separate steps.

## The project is a library

CLI packaging requires `[package].type = "bin"`. Use [mobile library packaging](mobile.md) for supplied mobile library files, or [publishing](publishing.md) to share Dream source.

See [CLI packaging](desktop.md) for output layouts and [command reference](../tooling/commands.md) for accepted arguments.

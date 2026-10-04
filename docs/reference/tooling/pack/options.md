# Packaging options and troubleshooting

Start with a working release build before changing packaging options.

| Option | Purpose |
| --- | --- |
| `--target windows-x64` | Package for a specific desktop platform and architecture |
| `--target all` | Build all supported desktop targets; each needs its own build requirements |
| `-O2` | Choose optimization level 2 instead of the default release level |
| `-p name` | Choose the package in a workspace |
| `[package].icon` | Set the app icon in `dream.toml` |

## A target cannot build

Run `dreamer toolchain doctor --target <target-triple>`. It reports missing build tools and libraries without installing them. Install the required tools using the [toolchain guide](../toolchain.md). An SDK may still be required for the target platform.

## A packaged app cannot start

Copy the entire target pack folder, including its accompanying libraries. On macOS, share the `.app` bundle. The receiving machine also needs the platform's system libraries. Signing and notarization are separate distribution steps.

## The project is a library

Desktop packaging requires `[package].type = "bin"`. Use [mobile library packaging](mobile.md) for supplied mobile library files, or [publishing](../publishing.md) to share Dream source.

See [desktop packaging](desktop.md) for output layouts and [command reference](../commands.md) for accepted arguments.

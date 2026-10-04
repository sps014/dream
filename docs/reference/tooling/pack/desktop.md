# Package a desktop app

Create a folder you can share with someone who does not have Dream installed. Keep the executable and its accompanying libraries together.

## Package your app

Builds the package natively and writes the executable to `target/pack/<os>-<arch>/<name>-<os>-<arch>`
(`.exe` on Windows). Browser and Node still load `.wasm`. The `[package].icon` is already compiled
into the executable; around it, each OS gets what it needs to show the app with that icon:

| Target | Output in `target/pack/<os>-<arch>/` |
|---|---|
| macOS | `<name>-macos-<arch>` with selected adjacent `libdream_host_*.dylib` libraries, and `<name>.app/Contents/{MacOS/<name>, Frameworks/, Info.plist, Resources/icon.icns}` with the same libraries in Frameworks |
| Linux | `<name>-linux-<arch>`, selected adjacent `libdream_host_*.so` libraries, `<name>.desktop`, and `<name>.png` when an icon is set |
| Windows | `<name>-windows-<arch>.exe` with the icon as its Explorer/taskbar icon, and selected adjacent `dream_host_*.dll` libraries |

Every native program ships the core library. Networking, GPU and WebView/desktop libraries are
included only when host calls from those packages survive compiler pruning. Unused imports and
CPU-only GPU helpers do not add networking or GUI libraries to a fresh package.

`Info.plist` takes its name and version from `[package]`, and the bundle id is
`dev.dream.<name>`. The `.desktop` entry names the executable and icon relative to the pack folder,
so copy them into `~/.local/share/applications` and an icon theme folder to install the app.

```bash
dreamer pack                         # --release / -O3 for the host
dreamer pack -O2                     # same `-O` / `--release` tokens as `dreamer run`
dreamer pack --target linux-arm64   # cross target with its capability libraries installed
dreamer pack --target all           # build each desktop target; all dependencies must be available
```

Each target builds in `target/<target-triple>/<debug|release>/` and publishes into its own pack directory. Desktop pack requires a bin package; mobile library packaging uses the slice workflow below. Copy the target pack folder, or the macOS `.app`, to
redistribute it: packed executables use loader-relative paths to their bundled capability libraries, not
the builder's toolchain directory. The macOS libraries have `@rpath` install names and
an ad-hoc signature; distribution signing/notarization is a separate step. Target machines
still need compatible OS libraries (including the Linux host runtime's system dependencies),
but do not need the Dream compiler or toolchain. Direct compiler builds can request the same
adjacent runtime layout with `dream --relocatable <file>`; ordinary build/run lookup is unchanged.

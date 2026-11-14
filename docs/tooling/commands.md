---
title: Command reference
summary: "Every `dreamer` command and flag."
icon: material/format-list-text
---

# Dreamer command reference

Look up a command here. For a walkthrough, use the guides linked from the [Dreamer overview](index.md).

## Command reference

| Command | Effect |
|---|---|
| `dreamer init [console\|desktop\|mobile\|app\|web\|node\|lib\|test\|workspace] [name] [--dir <path>] [--output staticlib\|cdylib]` | Create a project from a template. Omit the template for a console project in the current directory; a name creates a new folder. `desktop` creates a GPU canvas, `mobile` declares Android/iOS, and `app` declares desktop plus mobile. Mobile canvas execution is pending. `--output` only applies to `lib`. See [Create a project](create-project.md). |
| `dreamer devices [--target android\|ios] [--json]` | List connected Android devices and available iOS simulators without booting them. |
| `dreamer add <name> [--version <req>] [--path <dir>] [--git <url> [--tag/--branch/--rev <ref>] [--subdir <dir>]] [--dev] [-p <name>]` | Add (or update) a dependency in `dream.toml`, then resolve and install. |
| `dreamer remove <name> [-p <name>]` | Remove a dependency from `dream.toml` and `dream_packages/`, then re-resolve. |
| `dreamer install` | Resolve `dream.toml` (respecting `dream.lock` where still compatible) and materialize `dream_packages/`. In a `[workspace]`, installs all members into the root lock/`dream_packages/`. |
| `dreamer update [<name>]` | Re-resolve to the latest compatible version(s); with a name, only that package is allowed to move. |
| `dreamer build [--target native\|web\|node\|android\|ios] [--release] [--profile \| --use-profile[=<path>]] [-p <name>]` | Install, then compile the package. An explicit target restricts the build to that host; canvas apps default to desktop when present. Mobile canvas packaging is pending. Wasm lands in `target/web/`; native binaries in `target/debug` or `target/release`. When `targets` includes `node`, also copies into `target/node/`. PGO flags: see [Profile-guided native builds](build-run.md#profile-guided-native-builds). |
| `dreamer run [--release] [--profile \| --use-profile[=<path>]] [--port <n>] [--target native\|web\|node\|android\|ios] [-p <name>] [-- <args>]` | Install, then run on the resolved host. Canvas apps default to desktop when present; mobile canvas execution is pending. `--release` uses the release profile. Web serves on port **8787** by default (override with `--port`); a second run restarts the previous server on that port. Errors on `type = "lib"`. |
| `dreamer test [--release] [--filter <substr>] [--native] [--node] [-p <name>]` | Install (incl. dev-deps), then build and run test projects. Each project's `@test` functions compile into one cached runner. Inside a package it runs that package (a `type = "test"` package, or its `tests/` folder); at a workspace root it runs every test project in parallel and prints one summary. `--native` / `--node` override `[test].targets`. See [Test a project](testing.md). |
| `dreamer pack [--release] [-O<lvl>] [--target <os>-<arch>]… [-p <name>]` | Build a **bin** package into a native executable → `target/pack/<os>-<arch>/<name>-<os>-<arch>[.exe]`. Pack is always a release build (optimization level 3); `-O` / `--optimize` picks another release level. Cross targets need target-built capability libraries and the target sysroot or platform SDK. Distinct from registry `publish`. |
| `dreamer publish [--registry <url>] [--token <tok>] [-p <name>]` | Package source (`dream.toml` + `src/`) and publish it to a registry (≤10 MiB). Rejects path-only dependencies. |
| `dreamer search <query>` | Search the registry by name / description / keywords. |
| `dreamer tree [-p <name>]` | Print the resolved dependency tree from `dream.lock`. |
| `dreamer toolchain install [binaryen\|slang]` | Download pinned host toolchains into `~/.dream/toolchains/`: `binaryen` (the WebAssembly optimizer), `slang` (the shader compiler generators can run). With no argument, installs `binaryen`. LLVM and mold ship with Dream releases. |
| `dreamer toolchain doctor [--target <android\|ios\|target-triple>] [--json]` | Check mobile SDKs and target libraries with `android` or `ios`, or print resolved compiler/linker paths, SDK arguments, capability libraries and configuration hash. Missing requirements produce a failure exit code; diagnosis never installs components. |
| `dreamer toolchain list` | Show which of those components are installed. |
| `dreamer toolchain uninstall <component>` | Remove that component. |

`dreamer toolchain install` installs tools. `dreamer install` installs packages.

Builds look for the compiler in `DREAM_LLVM`, then the release bundle, then `~/.dream/toolchains/llvm-*`. Linux and macOS look for the linker in `DREAM_MOLD`, then the release bundle, then `~/.dream/toolchains/mold-*`; Windows uses the bundled linker, and nothing is taken from `PATH`.

`use-toolchain.sh` fetches the development compiler and linker when they are missing. `DREAM_SKIP_LLVM=1` skips that download.

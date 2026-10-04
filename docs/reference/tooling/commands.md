# Dreamer command reference

Look up a command here. For a walkthrough, use the guides linked from the [Dreamer overview](dreamer.md).

## Command reference

| Command | Effect |
|---|---|
| `dreamer init [name] [--lib] [--runtime native,web,node] [--dir <path>]` | Scaffold `dream.toml` + source stub (`src/main.dream` for bins, `src/<name>.dream` for `--lib`), `.gitignore` (`dream_packages/`, `target/`), and (when `--runtime` includes them) `index.html` / `run.mjs` linked to stable `target/web/` / `target/node/` aliases. |
| `dreamer add <name> [--version <req>] [--path <dir>] [--git <url> [--tag/--branch/--rev <ref>]] [--dev] [-p <name>]` | Add (or update) a dependency in `dream.toml`, then resolve and install. |
| `dreamer remove <name> [-p <name>]` | Remove a dependency from `dream.toml` and `dream_packages/`, then re-resolve. |
| `dreamer install` | Resolve `dream.toml` (respecting `dream.lock` where still compatible) and materialize `dream_packages/`. In a `[workspace]`, installs all members into the root lock/`dream_packages/`. |
| `dreamer update [<name>]` | Re-resolve to the latest compatible version(s); with a name, only that package is allowed to move. |
| `dreamer build [--release] [--profile \| --use-profile[=<path>]] [-p <name>]` | Install, then compile the package. Wasm lands in `target/web/`; native binaries in `target/debug` or `target/release`. When `targets` includes `node`, also copies into `target/node/`. PGO flags: see [Profile-guided native builds](build-run.md#profile-guided-native-builds). |
| `dreamer run [--release] [--profile \| --use-profile[=<path>]] [--port <n>] [--target native\|web\|node] [-p <name>] [-- <args>]` | Install, then run on the resolved host (see below). `--release` uses the release profile. Web serves on port **8787** by default (override with `--port`); a second run restarts the previous server on that port. Errors on `type = "lib"`. |
| `dreamer test [--release] [--filter <substr>] [-p <name>]` | Install (incl. dev-deps), then run `dream test tests/` — discovers `@test` functions under the project's `tests/` directory. |
| `dreamer pack [--release] [-O<lvl>] [--target <os>-<arch>]… [-p <name>]` | Build a **bin** package into a native executable → `target/pack/<os>-<arch>/<name>-<os>-<arch>[.exe]`, plus a macOS `.app` or Linux `.desktop` entry. Default is `--release` (optimization level 3); `-O` / `--optimize` override like `dreamer run`. Cross targets use Zig plus target-built capability libraries and any required platform SDK. Distinct from registry `publish`. |
| `dreamer publish [--registry <url>] [--token <tok>] [-p <name>]` | Package source (`dream.toml` + `src/`) and publish it to a registry (≤10 MiB). Rejects path-only dependencies. |
| `dreamer search <query>` | Search the registry by name / description / keywords. |
| `dreamer tree [-p <name>]` | Print the resolved dependency tree from `dream.lock`. |
| `dreamer toolchain install [llvm\|cc\|wasi-sdk]` | Download pinned host toolchains into `~/.dream/toolchains/`: `llvm` (the code generator every build needs), `cc` (Zig, the native linker driver when there is no system `cc`), `wasi-sdk` (wasm32 runtime and linker). With no argument, installs every component available for the host. |
| `dreamer toolchain doctor [--target <target-triple>] [--json]` | Print resolved compiler/linker paths, SDK arguments, capability libraries and configuration hash. Missing requirements produce a failure exit code; diagnosis never installs components. |
| `dreamer toolchain list` | Show which of those components are installed. |
| `dreamer toolchain uninstall <component>` | Remove that component. |

`dreamer toolchain install` is **not** `dreamer install` (packages). Every build uses the pinned LLVM from `DREAM_LLVM`, then `~/.dream/toolchains/llvm-*`. Native builds link with `DREAM_CC` / `CC`, then the installed Zig, then `cc` / `clang` on `PATH`. The public installer (`install.sh` / `install.ps1`) and `use-toolchain.sh` run `dreamer toolchain install llvm` when LLVM is missing (`DREAM_SKIP_LLVM=1` skips it) and `dreamer toolchain install cc` when no C compiler is found (`DREAM_SKIP_CC=1` skips it).

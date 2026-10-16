---
title: Workspaces
summary: Develop related packages with shared dependencies.
icon: material/folder-multiple-outline
---

# Work with several packages

A workspace lets related packages share installed dependencies and one lockfile. Use it for an app and its supporting libraries.

## Workspaces (monorepos)

A repo can hold multiple packages behind one root `dream.toml`:

```toml
# repo root
[workspace]
members = ["packages/greeter", "apps/cli"]
```

Each member keeps a normal package `dream.toml` (with `[package]`). Path deps between members work
as today:

```toml
# apps/cli/dream.toml
[package]
name = "cli"
version = "0.1.0"
type = "bin"
entry = "src/main.dream"
targets = ["native"]

[dependencies]
greeter = { version = "0.1.0", path = "../../packages/greeter" }
```

Behavior:

- One `dream.lock` and one `dream_packages/` at the **workspace root**.
- `dreamer install` (from the root or any member) resolves **all** members’ deps into that shared
  install, then symlinks each member’s `dream_packages/` → the root so imports keep
  working with no special config.
- Package selection: inside a member directory, commands target that package; at the virtual
  workspace root, pass `-p` / `--package <name>` for `build` / `run` / `test` / `pack` / `publish`
  / `add` / `remove` / `tree`.

```bash
dreamer install
dreamer run -p cli
cd apps/cli && dreamer run          # same package, no -p
dreamer publish -p greeter           # one package at a time
```

### Test projects in a workspace

Test packages (`type = "test"`) are ordinary members. Keep each suite next to the code it tests or
under a shared `tests/` folder:

```toml
[workspace]
members = ["packages/greeter", "apps/cli", "tests/greeter", "tests/fixtures"]
```

```toml
# tests/greeter/dream.toml
[package]
name = "greeter_tests"
version = "0.1.0"
type = "test"

[dependencies]
greeter = { path = "../../packages/greeter" }

[test]
targets = ["native", "node"]
```

`dreamer test` at the workspace root runs every test project — test packages, plus members that
keep tests in a `tests/` folder — in parallel and prints one summary. `dreamer test -p
greeter_tests` runs one; inside a member, `dreamer test` runs that member's tests.

| Where you run `dreamer test` | What runs |
|---|---|
| Workspace root, no `-p` | Every test package and every member with a `tests/` folder, in parallel |
| Anywhere, with `-p <name>` | That member's tests |
| Inside a member folder | That member's tests |

`dreamer init myapp --workspace` scaffolds this layout: a root `[workspace]`, an `app/` program
and a `tests/` test project that depends on it.

**Runtime host** vs **pack triple** vs **compiler target**:

| Setting | Values | Chooses |
|---|---|---|
| `[package].targets`, `dreamer run --target` | `native`, `web`, `node` | Which host runs the app |
| `dreamer pack --target` | `linux-x64`, `macos-arm64`, … or `all` | Which OS/arch executable to package; see [CLI packaging](../deploy/desktop.md) |
| `dream --runtime-target` | `native`, `web`, `node` | Which runtime's API availability is checked |
| `dream --target` | An LLVM target triple, such as `aarch64-unknown-linux-gnu` | What the compiler compiles and links for |
| `dream --object` (with `--target`) | — | Stop at an unlinked object; no platform SDK needed |

Foreign executables cannot be run by `dream run`, `test`, or the debug adapter.

Cross builds read these environment variables:

| Variable | Meaning |
|---|---|
| `DREAM_CC` | C compiler driver for linking; checked before `CC` |
| `CC` | C compiler driver when `DREAM_CC` is unset |
| `DREAM_TARGETS` | Root of target-built capability libraries (default `~/.dream/targets`) |
| `DREAM_SYSROOT` | Target SDK sysroot |
| `SDKROOT` | Apple SDK path |
| `DEVELOPER_DIR` | Xcode installation to use |

Cross builds use the installed Zig unless `DREAM_CC` / `CC` names a target-capable compiler.
Install Zig with `dreamer toolchain install cc`. Place target-built capability libraries in
`~/.dream/targets/<target-triple>/lib/` (override the targets root with `DREAM_TARGETS`),
including Windows import libraries where applicable. The compiler validates architecture,
file format and the current library version marker before linking; it never substitutes host libraries.
Set `DREAM_SYSROOT` to the target SDK sysroot, or `SDKROOT` for Apple SDKs. `DEVELOPER_DIR` selects an Xcode installation. On macOS,
`xcrun` resolves the selected macOS/iOS SDK. Android builds can select the NDK Clang driver
with `DREAM_CC`. Cross builds require the target runtime sources and the required build-tools installation.

Use `dreamer toolchain doctor --target aarch64-unknown-linux-gnu` to inspect these requirements.
Windows icon resources use pinned `llvm-rc` or the selected/installed Zig resource compiler.
The configuration hash and resolved tool/library identities, including resolved SDK arguments, enter build stamps, so changing
the compiler, SDK or capability libraries invalidates cached outputs. Cross-target PGO requires
running profiles on the target and is currently rejected.

### LSP

No extra setup. The language server already uses the nearest member `dream.toml` (lib vs bin
CodeLens) and that member’s `dream_packages/` symlink for import completion.

### Publishing from a monorepo

Publish is always a **single member** (`dreamer publish -p greeter`, or `cd` into the member). The
tarball still contains only that package’s `dream.toml` + `src/` + README — not siblings.

Path-only deps (`greeter = { path = "..." }` with no version) cannot be resolved by registry
consumers; `dreamer publish` errors and asks you to write:

```toml
greeter = { version = "0.1.0", path = "../../packages/greeter" }
```

Install still prefers the path locally; the published index records the version requirement.
Publish leaf libraries before apps that depend on them.

See `sample/monorepo/` for a complete layout.

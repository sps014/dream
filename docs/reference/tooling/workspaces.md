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

**Runtime host** vs **pack triple** (unchanged naming):

- `package.targets` / `dreamer run --target native|web|node` — which host runs the app
- `dreamer pack --target macos-arm64` — which OS/arch executable to build

The compiler CLI uses separate flags: `dream --runtime-target native|web|node` selects
runtime availability for checks before building. `dream --target <target-triple>` compiles and links
for that target. Add `--object` to stop at an unlinked object and its build description,
without a platform SDK. Foreign executables cannot be run by `dream run`, `test`, or the debug adapter.

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

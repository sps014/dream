# Package Manager (`dreamer`)

`dreamer` manages Dream projects: it reads `dream.toml`, pins dependencies in `dream.lock`,
installs them into `dream_packages/`, and wraps `build` / `run` / `test` / `pack`.

Install it with the [quickstart](../../learn/quickstart.md) installer — you get `dream` and
`dreamer` together. Then:

```bash
dreamer init hello
cd hello
dreamer run
```

## Installing `dreamer`

The installer puts `dream` and `dreamer` on your PATH (`~/.dream/bin`). Open a new terminal and
check with `dreamer --help`.

If an editor cannot find them, set `dream.home` / `dreamer.home` to the directory that contains
the binaries (the VS Code extension uses those settings).

## The manifest: `dream.toml`

Every Dream project managed by `dreamer` has a `dream.toml` at its root, analogous to `Cargo.toml`
or `pyproject.toml`:

```toml
[package]
name = "myapp"
version = "0.1.0"
type = "bin"                    # or "lib" (default: bin)
edition = "2026"
authors = ["Jane Doe <jane@example.com>"]
description = "My Dream app"
entry = "src/main.dream"        # required for bin; forbidden for lib
license = "MIT"
keywords = ["http", "json"]         # optional; used by dreamer search / the registry site
targets = ["native", "web"]     # optional hosts: native, web, node (omit = no preference)
icon = "assets/icon.png"        # optional PNG; packed into single-file exe by `dreamer pack`

[dependencies]
http-utils = "1.2"                                    # semver requirement, resolved from a registry
json-tools = { version = "0.3", registry = "default" }
local-lib  = { path = "../local-lib" }                 # local path dependency
vendored   = { git = "https://github.com/user/vendored-dream", tag = "v1.0.0" }

[dev-dependencies]
test-utils = "0.4"

[scripts]
start = "dreamer run"

[registries]
default = "https://raw.githubusercontent.com/sps014/dream-registry/main"
```

- `[package].type` is `bin` (default) or `lib`. Libraries omit `entry`, are not runnable (`dreamer run` /
  `dreamer pack` error), and are typechecked via the conventional `src/<import_segment>.dream` root
  (`http-utils` → `src/http_utils.dream`, `foo.bar` → `src/foo_bar.dream`). Binaries require `entry` and a top-level `main`.
- `[package].entry` is the file `dreamer build` / `dreamer run` compile (**bin only**).
- Package builds emit wasm under `target/web/` (debug and `--release` share that folder) and native C
  under `target/debug/` or `target/release/`. Bare `dream file.dream` (no enclosing `dream.toml`)
  still uses `<source-dir>/target/web/` (wasm) or `target/debug|release/` (native C) — never siblings
  next to the `.dream` file.
- **Node hosts** also get `target/node/` (copied from `target/web/` by `dreamer build` / `dreamer run`
  when `targets` includes `node`). Scaffolded `index.html` / `run.mjs` import `target/web/` /
  `target/node/` — no need to edit them when switching debug ↔ release. Existing projects that
  hardcode `target/debug/…` should retarget once to `target/web/` / `target/node/`.
- `[package].targets` is an optional list of hosts this project supports: `native`
  (`dream run`), `web` (browser + `*.web.runtime.js`), and/or `node` (Node ≥ 18 + `*.node.runtime.js`).
  Omit the field (or leave it empty) for today's free-choice behavior — `dreamer run` defaults to
  native. Combinations are allowed; see `dreamer run` below for how the host is chosen.
- `[package].icon` is an optional path to a PNG (relative to the `dream.toml` directory).
  - **`dream run`**: loads the file from disk when a GPU window is created.
  - **`dreamer pack`**: copies the PNG into the single-file native executable so no `assets/`
    folder is required next to the `.exe`. Web still uses a static file / favicon link.
- A dependency is either a bare semver requirement string, or a table with exactly one of
  `path`, `git`, or `version` (+ optional `registry`).
- Package names must start with a letter and may contain ASCII letters, digits, `-`, `_`, and `.`.
  The registry identity is always the full name string as written in `dream.toml` / `dream.lock`
  (e.g. `json-tools`, `foo.bar`). On disk and in `import` statements, hyphens and dots map to
  underscores: `json-tools` → `import json_tools;`, `foo.bar` → `import foo_bar;`. A dotted
  `import` path still means a subpath inside a package (`import json_tools.parse;` →
  `dream_packages/json_tools/src/parse.dream`), never a registry name with a dot — so the registry
  package `foo.bar` is always imported as `import foo_bar;`, not `import foo.bar;`.
- `[registries]` maps registry aliases to base URLs; a dependency's `registry = "..."` picks one,
  defaulting to the `default` alias.
- `[scripts]` is currently informational project metadata — no `dreamer` subcommand executes it
  yet, but it's a stable place to document how a project is normally built/run/tested.

## The lockfile: `dream.lock`

`dreamer install` (and every command that implies it) writes `dream.lock`: the exact, pinned
dependency graph, analogous to `Cargo.lock`/`package-lock.json`. It should be committed to version
control for applications so every checkout resolves to byte-identical dependency versions.

```toml
version = 1

[[package]]
name = "json-tools"
version = "0.3.1"
source = "registry+https://raw.githubusercontent.com/sps014/dream-registry/main"
checksum = "sha256:9f2c...ab31"
dependencies = []
```

`source` is one of `registry+<url>`, `git+<url>#<rev>`, or `path+<path>`. Packages are always
written sorted by name so the file diffs cleanly and resolution order never affects its contents.

Re-running `dreamer install` prefers versions already pinned in an existing `dream.lock` (as long
as they still satisfy every requirement in `dream.toml`), so it never silently upgrades a
dependency just because a newer version was published — use `dreamer update` for that.

## `dream_packages/`

Every dependency is materialized under `dream_packages/<import_segment>/` next to
`dream.toml` — a local path dependency is symlinked (so edits show up immediately), while registry
and git dependencies are copied from a shared, checksum-verified download cache at
`~/.dream/registry/`. `dream_packages/` is never committed (`dreamer init` adds it to
`.gitignore`); it's fully reproducible from `dream.toml` + `dream.lock`.

When a plain `import` doesn't resolve to a local file, Dream looks under `dream_packages/`:

- `import json_tools;` (no dot) looks for `dream_packages/json_tools/src/json_tools.dream` — a
  package's self-named entry file.
- `import json_tools.parse;` looks for `dream_packages/json_tools/src/parse.dream`.

See [Imports & Modules](../language/imports.md) for the base (non-package) import syntax.

The LSP suggests installed package names when you type `import `, reading from `dream_packages/` —
no separate configuration needed.

## Registry protocol

A registry is a sparse index plus tarball storage. A plain directory works, served as a local
`file://` path or over any static HTTP file server:

```text
<base>/index/<name>                         newline-delimited JSON, one entry per published version
<base>/dl/<name>/<name>-<version>.tar.gz    the tarball an index entry's "tarball" field points at
```

`<name>` is the full registry package name (including `.` when present), e.g. `index/foo.bar` and
`dl/foo.bar/foo.bar-1.0.0.tar.gz`. Dots are literal filename characters, not path separators.

Each line of `<base>/index/<name>` is a JSON object:

```json
{
  "name": "json-tools",
  "vers": "0.3.1",
  "deps": [{"name": "buffer-utils", "req": "^1.0"}],
  "cksum": "sha256:...",
  "tarball": "dl/json-tools/json-tools-0.3.1.tar.gz",
  "description": "JSON helpers",
  "authors": ["Jane Doe <jane@example.com>"],
  "license": "MIT",
  "edition": "2026",
  "type": "lib",
  "targets": ["native", "web"],
  "readme": "README.md",
  "keywords": ["json", "parse"]
}
```

Fields beyond `name` / `vers` / `deps` / `cksum` / `tarball` are optional metadata copied from
`dream.toml` at publish time. `readme` is an **archive-relative path** (e.g. `README.md`) pointing at
the README packed into the tarball — the index never embeds README body text.
`keywords` come from `[package].keywords` and feed static search.
`catalog.json` stores the same discovery fields except `deps` / `cksum` / `tarball`.

Optional endpoints / files for `dreamer search` / `dreamer publish`:

- `GET  <base>/search?q=<query>` → JSON array of index-entry objects (dynamic registries)
- `GET  <base>/catalog.json` → compact search catalog used when `/search` is absent (static/GitHub registries)
- `POST <base>/api/v1/publish` → JSON body `{ "entry": <index-entry>, "tarball_base64": "..." }` (non-GitHub HTTP registries)

The default public registry is the GitHub repo
[`sps014/dream-registry`](https://github.com/sps014/dream-registry), served at
`https://raw.githubusercontent.com/sps014/dream-registry/main`. Indexes live under `index/`,
tarballs under `dl/` (separate trees). Max package tarball size is **10 MiB**.

`dreamer publish` to that registry uses the GitHub Contents API (set `DREAM_REGISTRY_TOKEN` or
`--token` with `contents:write`). Point `[registries] default` at any other `file://` or
`http(s)://` location implementing the protocol above for private/offline use.

### Finding packages

- **CLI:** `dreamer search <query>` — matches package name, description, and keywords.
- **Web:** [sps014.github.io/dream-registry](https://sps014.github.io/dream-registry/) — browse and copy install commands.

The first published library is [`semver`](https://github.com/sps014/dream-packages/tree/main/semver)
(`dreamer add semver`). Official libraries live in [`sps014/dream-packages`](https://github.com/sps014/dream-packages).

## Dependency resolution

Registry dependencies resolve to the highest version that satisfies every accumulated requirement.
`path` and `git` dependencies are pinned from their own `dream.toml` and are never subject to
registry version selection. Conflicting requirements produce a clear error naming both sides.

## Command reference

| Command | Effect |
|---|---|
| `dreamer init [name] [--lib] [--runtime native,web,node] [--dir <path>]` | Scaffold `dream.toml` + source stub (`src/main.dream` for bins, `src/<name>.dream` for `--lib`), `.gitignore` (`dream_packages/`, `target/`), and (when `--runtime` includes them) `index.html` / `run.mjs` linked to stable `target/web/` / `target/node/` aliases. |
| `dreamer add <name> [--version <req>] [--path <dir>] [--git <url> [--tag/--branch/--rev <ref>]] [--dev] [-p <name>]` | Add (or update) a dependency in `dream.toml`, then resolve and install. |
| `dreamer remove <name> [-p <name>]` | Remove a dependency from `dream.toml` and `dream_packages/`, then re-resolve. |
| `dreamer install` | Resolve `dream.toml` (respecting `dream.lock` where still compatible) and materialize `dream_packages/`. In a `[workspace]`, installs all members into the root lock/`dream_packages/`. |
| `dreamer update [<name>]` | Re-resolve to the latest compatible version(s); with a name, only that package is allowed to move. |
| `dreamer build [--release] [-p <name>]` | Install, then compile the package. Wasm lands in `target/web/`; native C in `target/debug` or `target/release`. When `targets` includes `node`, also copies into `target/node/`. |
| `dreamer run [--release] [--port <n>] [--target native\|web\|node] [-p <name>] [-- <args>]` | Install, then run on the resolved host (see below). `--release` uses the release profile. Web serves on port **8787** by default (override with `--port`); a second run restarts the previous server on that port. Errors on `type = "lib"`. |
| `dreamer test [--release] [--filter <substr>] [-p <name>]` | Install (incl. dev-deps), then run `dream test tests/` — discovers `@test` functions under the project's `tests/` directory. |
| `dreamer pack [--release] [-O<lvl>] [--target <os>-<arch>\|all]… [-p <name>]` | Build a **bin** package into a single native executable → `target/pack/<name>-<os>-<arch>[.exe]`. Default is `--release` (cc `-O3`); `-O` / `--optimize` override like `dreamer run`. Default target is the host OS/arch. Distinct from registry `publish`. |
| `dreamer publish [--registry <url>] [--token <tok>] [-p <name>]` | Package source (`dream.toml` + `src/`) and publish it to a registry (≤10 MiB). Rejects path-only dependencies. |
| `dreamer search <query>` | Search the registry by name / description / keywords. |
| `dreamer tree [-p <name>]` | Print the resolved dependency tree from `dream.lock`. |
| `dreamer toolchain install [cc]` | Download a pinned C compiler (Zig) for native C builds into `~/.dream/toolchains/`. |
| `dreamer toolchain list` | Show which of those components are installed. |
| `dreamer toolchain uninstall cc` | Remove that component. |

`dreamer toolchain install` is **not** `dreamer install` (packages). `dream run` (native C) uses `DREAM_CC` / `CC`, then the installed Zig, then `cc` / `clang` on `PATH`. The public installer (`install.sh` / `install.ps1`) and `use-toolchain.sh` run `dreamer toolchain install cc` when none of those are found (`DREAM_SKIP_CC=1` skips it).

### Native `dreamer pack`

Produces a single native executable per selected platform and writes it to
`target/pack/<name>-<os>-<arch>`. Browser and Node still load `.wasm`. No project `assets/`
folder is required next to the packed binary.

```bash
dreamer pack                         # --release / -O3; host → target/pack/<name>-<os>-<arch>
dreamer pack -O2                     # same `-O` / `--release` tokens as `dreamer run`
dreamer pack --target linux-x64
dreamer pack --target macos-arm64 --target windows-x64
dreamer pack --target all            # linux/macos/windows × x64/arm64
```

Cross-compiling to another OS/arch needs a working linker for that target; failures
are reported (targets are never silently skipped). Libraries cannot be packed.

### How `dreamer run` picks a host

| `package.targets` | No `--target` | With `--target X` |
|---|---|---|
| empty / omitted | **native** (`dream run`) | any of `native` / `web` / `node` (ad-hoc escape hatch) |
| exactly one entry | that host | must match that host |
| two or more | error — require `--target` | `X` must be one of the listed targets |

Per host:

- **native** — `dream run [--release] <entry> [args…]`.
- **node** — write `run.mjs` from `package.entry` if it is missing, compile with `--runtime --node`
  (refreshing `target/node/`), then `node run.mjs`.
- **web** — compile with `--runtime --web` (wasm in `target/web/`), then serve the project root on
  `http://127.0.0.1:8787/index.html` by default (colored log; Ctrl-C to stop). A later
  `dreamer run --target web` restarts that server on the same port. Override with `--port`.

Use `dreamer run --release` (optionally with `--target`) so release artifacts feed the same stable
alias paths the scaffolds already reference.

## Workspaces (monorepos)

A repo can hold multiple packages behind one root `dream.toml`:

```toml
# repo root
[workspace]
members = ["packages/shared", "apps/cli"]
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
shared = { version = "0.1.0", path = "../../packages/shared" }
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
dreamer publish -p shared           # one package at a time
```

**Runtime host** vs **pack triple** (unchanged naming):

- `package.targets` / `dreamer run --target native|web|node` — which host runs the app
- `dreamer pack --target macos-arm64` — which OS/arch executable to embed

### LSP

No extra setup. The language server already uses the nearest member `dream.toml` (lib vs bin
CodeLens) and that member’s `dream_packages/` symlink for import completion.

### Publishing from a monorepo

Publish is always a **single member** (`dreamer publish -p shared`, or `cd` into the member). The
tarball still contains only that package’s `dream.toml` + `src/` + README — not siblings.

Path-only deps (`shared = { path = "..." }` with no version) cannot be resolved by registry
consumers; `dreamer publish` errors and asks you to write:

```toml
shared = { version = "0.1.0", path = "../../packages/shared" }
```

Install still prefers the path locally; the published index records the version requirement.
Publish leaf libraries before apps that depend on them.

See `sample/monorepo/` for a complete layout.

## Walkthrough

```bash
# scaffold a new project (optional hosts)
dreamer init myapp --runtime web,node && cd myapp

# add a dependency from the default registry
dreamer add json-tools --version "^0.3"

# add a local sibling project during development
dreamer add local-lib --path ../local-lib

# install everything into dream_packages/, generate dream.lock
dreamer install
```

```dream
// src/main.dream
import json_tools;
import local_lib;
import system;

fun main(): void {
    System.println(hello());   // from json_tools/src/json_tools.dream
}
```

```bash
# compile and run (multi-target projects need --target)
dreamer run --target node
dreamer run --target web

# see what's actually installed
dreamer tree

# bump json-tools to the newest version satisfying dream.toml
dreamer update json-tools

# publish this project itself to the default GitHub registry
dreamer publish
# or: export DREAM_REGISTRY_TOKEN=ghp_... && dreamer publish --registry https://raw.githubusercontent.com/sps014/dream-registry/main
```

## Trying it end to end without a hosted registry

Since a plain directory is a fully compliant registry, you can try the whole flow locally:

```bash
mkdir -p /tmp/my-registry
# ... publish a package into it, e.g. by running `dreamer publish` from that package's own
# project with `--registry file:///tmp/my-registry` ...

# then, in a consuming project's dream.toml:
# [registries]
# default = "file:///tmp/my-registry"
dreamer add that-package
```

## For contributors

Import resolution, the registry protocol wire format, and resolver edge cases live in the
[Contributing](../../internals/README.md) handbook and the `tooling/dreamer` crate — not required for
day-to-day package use.

# Project settings

Use `dream.toml` to describe your app, its dependencies, and the environments it supports.

## The manifest: `dream.toml`

`dream.toml` is the project file at the root of every Dream project managed by `dreamer`:

```toml
[package]
name = "myapp"
version = "0.1.0"
type = "bin"                    # "bin", "lib" or "test" (default: bin)
edition = "2026"
authors = ["Jane Doe <jane@example.com>"]
description = "My Dream app"
entry = "src/main.dream"        # required for bin; forbidden for lib
license = "MIT"
keywords = ["http", "json"]         # optional; used by dreamer search / the registry site
targets = ["native", "web"]     # optional hosts: native, web, node (omit = no preference)
icon = "assets/icon.png"        # optional PNG app icon for windows, the Dock and packed apps
assets = ["data", "config.json"] # optional files copied next to the build output (bin and test)

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

- `[package].type` is `bin` (default), `lib` or `test`.
  - Libraries omit `entry`, are not runnable (`dreamer run` / `dreamer pack` error), and are
    typechecked via the conventional `src/<import_segment>.dream` root
    (`http-utils` → `src/http_utils.dream`, `foo.bar` → `src/foo_bar.dream`).
  - Binaries require `entry` and a top-level `main`.
  - Test packages omit `entry`, `targets` and `icon`. Every `.dream` file in them is part of the
    suite, run with `dreamer test` (they cannot be built, run, packed or published). An optional
    `[test]` table sets the hosts the suite runs on:

    ```toml
    [test]
    targets = ["native", "node"]  # default: ["native"]
    ```

    `[test]` accepts only `targets`, and only on test packages. See [Test a project](testing.md).
- `[package].assets` lists files and folders, relative to the `dream.toml` directory, that the
  program reads at run time. Allowed on `bin` and `test` packages (an error on `lib`).
  - They are copied into the output folder: `target/debug` or `target/release` for native,
    `target/web` and `target/node` for wasm, `target/test/<profile>` for test runners, and next
    to the executable in `dreamer pack` output.
  - A program with assets runs with its output folder as the working directory, so it opens them
    with the same relative paths it uses in the project (`File.open("data/words.txt")`).
  - Paths must stay inside the project: no absolute paths and no `..`.
- `[package].entry` is the file `dreamer build` / `dreamer run` compile (**bin only**).
- Package builds emit wasm under `target/web/` (debug and `--release` share that folder) and native
  binaries under `target/debug/` or `target/release/`.
  - Bare `dream file.dream` (no enclosing `dream.toml`) still uses `<source-dir>/target/web/`
    (wasm) or `target/debug|release/` (native) — never siblings next to the `.dream` file.
- **Node hosts** also get `target/node/` (copied from `target/web/` by `dreamer build` / `dreamer run`
  when `targets` includes `node`).
  - Scaffolded `index.html` / `run.mjs` import `target/web/` / `target/node/` — no need to edit
    them when switching debug ↔ release.
  - Existing projects that hardcode `target/debug/…` should retarget once to `target/web/` /
    `target/node/`.
- `[package].targets` is an optional list of hosts this project supports: `native`
  (`dream run`), `web` (browser + `*.web.runtime.js`), and/or `node` (Node ≥ 18 + `*.node.runtime.js`).
  - Omit the field (or leave it empty) for today's free-choice behavior — `dreamer run` defaults to
    native.
  - Combinations are allowed; see `dreamer run` below for how the host is chosen.
- `[package].icon` is an optional path to a PNG (relative to the `dream.toml` directory). It is
  the only place an app icon is configured.
  - `dreamer build`, `run` and `pack` compile it into native binaries, so no file is read at run
    time. Windows executables use the embedded icon.
  - `dreamer` checks that the file exists and is a PNG, and warns when it is not square or is
    smaller than 256x256.
  - See [CLI packaging](pack/desktop.md) for native output layouts.
  - Web builds use it as the favicon that `dreamer run --target web` serves.
- A dependency is either a bare semver requirement string, or a table with exactly one of
  `path`, `git`, or `version` (+ optional `registry`).
- Package names must start with a letter and may contain ASCII letters, digits, `-`, `_`, and `.`.
  - The registry identity is always the full name string as written in `dream.toml` / `dream.lock`
    (e.g. `json-tools`, `foo.bar`).
  - On disk and in `import` statements, hyphens and dots map to underscores:
    `json-tools` → `import json_tools;`, `foo.bar` → `import foo_bar;`.
  - A dotted `import` path still means a subpath inside a package
    (`import json_tools.parse;` → `dream_packages/json_tools/src/parse.dream`), never a registry
    name with a dot — so the registry package `foo.bar` is always imported as `import foo_bar;`,
    not `import foo.bar;`.
- `[registries]` maps registry aliases to base URLs; a dependency's `registry = "..."` picks one,
  defaulting to the `default` alias.
- `[scripts]` is currently informational project metadata — no `dreamer` subcommand executes it
  yet, but it's a stable place to document how a project is normally built/run/tested.

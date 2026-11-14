---
title: Project settings
summary: "Describe your app and its environments in `dream.toml`."
icon: material/file-cog-outline
---

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
charts     = { git = "https://github.com/user/dream-libs", branch = "main", subdir = "charts" }

[dev-dependencies]
test-utils = "0.4"

[scripts]
start = "dreamer run"

[registries]
default = "https://raw.githubusercontent.com/sps014/dream-registry/main"
```

## Sections

Canvas templates add an `[app]` table and declare their platforms in `package.targets`:

```toml
[package]
name = "counter"
version = "0.1.0"
type = "bin"
entry = "src/main.dream"
targets = ["native", "android", "ios"]

[app]
id = "org.example.appcounter"
android-min-sdk = 26
ios-min-os = "16.0"
```

Change `app.id` from the generated placeholder to your application's identifier: at least two dot-separated segments, each starting with a letter, containing only letters or digits, and not a Java reserved word.

Android's minimum SDK must be at least 26, and iOS's minimum OS must be at least 16.0. Omitted fields use those defaults, and the table is valid only on a `bin` package targeting `native`, `android`, or `ios` (mobile targets require it).

An app that lists `native` selects it by default for build and run. Mobile-only apps need `--target android` or `--target ios` ([Mobile apps](../deploy/mobile.md)).

A manifest needs `[package]`, `[workspace]`, or both.

| Section | Purpose |
|---|---|
| `[package]` | Name, version, kind and metadata of this package |
| `[dependencies]` | Packages your code imports |
| `[dev-dependencies]` | Extra packages installed for tests and local work, never published as requirements |
| `[registries]` | Registry aliases mapped to base URLs |
| `[scripts]` | Named commands kept as project notes; `dreamer` does not run them |
| `[test]` | Hosts a test package runs on (`type = "test"` only) |
| `[lib]` | Native library kind, `output-type = "staticlib"` or `"cdylib"` (`type = "lib"` only); see [Native libraries](../deploy/native-libraries.md) |
| `[workspace]` | Member package folders; see [Workspaces](workspaces.md) |
| `[native.<set>]` | C/C++ sources and build flags; see [C interop](../language/interop/c.md) |
| `[[generators]]` | Compile-time source generators; see [Generators](../language/metaprogramming/generators.md) |

## `[package]` keys

| Key | Required | Default | Meaning |
|---|---|---|---|
| `name` | yes | — | Package name: starts with a letter; ASCII letters, digits, `-`, `_`, `.` |
| `version` | yes | — | Semantic version, such as `1.2.3` |
| `type` | no | `bin` | `bin`, `lib` or `test` |
| `entry` | `bin` only | — | Source file to compile; not allowed on `lib` or `test` |
| `edition` | no | none | Language edition, such as `2026` |
| `authors` | no | `[]` | Author strings, published to the registry |
| `description` | no | none | One-line summary, published and searched |
| `license` | no | none | License identifier, published to the registry |
| `keywords` | no | `[]` | Search words for `dreamer search` and the registry site |
| `targets` | no | `[]` | Hosts: `native`, `web`, `node`; canvas projects can also declare `android` and `ios` (execution pending). Not allowed on `test`. |
| `icon` | no | none | PNG app icon path; not allowed on `test` |
| `assets` | no | `[]` | Files and folders copied next to the output; not allowed on `lib` |
| `links` | no | none | Name of the native library this package provides; see [C interop](../language/interop/c.md) |

## Dependency keys

A dependency is a bare version requirement string, or a table with these keys:

| Key | Meaning |
|---|---|
| `version` | Semver requirement resolved from a registry |
| `registry` | Registry alias from `[registries]` (default: `default`) |
| `path` | Local folder; may be combined with `version` so the package can still be published |
| `git` | Git repository URL |
| `tag`, `branch`, `rev` | Which Git revision to use; at most one, and only with `git` |
| `subdir` | Folder inside the `git` repository that contains the package's `dream.toml`, for a repository that holds several packages; relative, without `..` |

`path` cannot be combined with `git` or `registry`, and `git` cannot be combined with `registry`.

Packages from one repository that holds several packages resolve against the same commit. Inside
such a repository, a package can name a sibling with a relative `path` (`json = { path = "../json" }`).
When the program also depends on that sibling through `git` and `subdir`, both refer to the same
package rather than conflicting.

## Details

- `[package].type` is `bin` (default), `lib` or `test`.
  - Libraries omit `entry`, are not runnable (`dreamer run` / `dreamer pack` error), and are
    typechecked via the conventional `src/<import_segment>.dream` root
    (`http-utils` → `src/http_utils.dream`, `foo.bar` → `src/foo_bar.dream`).
  - Binaries require `entry` and a top-level `main`.
  - Test packages omit `entry`, `targets`, and `icon`. Every `.dream` file is part of the suite and runs with `dreamer test` (they cannot be built, run, packed, or published), and an optional `[test]` table sets the hosts:

    ```toml
    [test]
    targets = ["native", "node"]  # default: ["native"]
    ```

    `[test]` accepts only `targets`, and only on test packages. See [Test a project](testing.md).
- `[package].assets` lists project-relative files and folders copied next to a `bin` or `test` program (an error on `lib`). The program starts in that folder, so `File.read("data/words.txt").await` uses the same path on desktop and mobile, and paths cannot be absolute or contain `..`.
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
- `[package].targets` lists hosts: `native` (`dream run`), `web` (browser + `*.web.runtime.js`), and `node` (Node ≥ 18 + `*.node.runtime.mjs`). Canvas projects with `[app]` can also declare `android` and `ios` ([Mobile apps](../deploy/mobile.md)).
  - Omit the field, or leave it empty, and `dreamer run` defaults to native.
  - Canvas projects that list `native` select it by default. Other combinations need an explicit run target ([Build and run](build-run.md)).
- `[package].icon` is an optional path to a PNG (relative to the `dream.toml` directory). It is
  the only place an app icon is configured.
  - `dreamer build`, `run` and `pack` compile it into native binaries, so no file is read at run
    time. Windows executables use the embedded icon.
  - `dreamer` checks that the file exists and is a PNG, and warns when it is not square or is
    smaller than 256x256.
  - See [CLI packaging](../deploy/desktop.md) for native output layouts.
  - Web builds use it as the favicon that `dreamer run --target web` serves.
- A dependency is either a bare semver requirement string, or a table using the
  [dependency keys](#dependency-keys) above.
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
- `[scripts]` records how you normally build, run, or test the project. No `dreamer` command runs those scripts.

## JavaScript dependencies

`[npm]` lists packages used by `jsimport`. The first import installs them into `.dream/npm` and writes `dream.npm.lock`. Commit the lock file. Installation scripts do not run. Add an `@types` package when the library ships no declarations. See [JavaScript interop](../language/interop/index.md).

```toml
[npm]
escape-string-regexp = "5.0.0"
```

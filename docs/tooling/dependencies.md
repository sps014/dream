---
title: Dependencies
summary: "Add, update, and import packages."
icon: material/package-variant-closed-plus
---

# Add and update packages

A package is reusable Dream code. Add a package to your project, then import the parts you need.

## Commands

| Command | Effect |
|---|---|
| `dreamer add <name>` | Add or replace a dependency in `dream.toml`, then install |
| `dreamer remove <name>` | Remove it from `[dependencies]` and `[dev-dependencies]` and from `dream_packages/`, then install |
| `dreamer install` | Resolve `dream.toml` into `dream.lock` and fill `dream_packages/` |
| `dreamer update [<name>]` | Move to the newest compatible versions; with a name, only that package moves |
| `dreamer tree` | Print the resolved dependency tree from `dream.lock` |
| `dreamer search <query>` | Search the registry by name, description and keywords |

`dreamer add` chooses the dependency source from its flags:

| Flag | Effect |
|---|---|
| `--version <req>` | Version requirement; without other source flags, `*` when omitted |
| `--path <dir>` | Local folder dependency (can be combined with `--version`) |
| `--git <url>` | Git dependency; cannot be combined with `--path` |
| `--tag`, `--branch`, `--rev` | Git revision to use; at most one, with `--git` (default: the repository's `HEAD`) |
| `--dev` | Add to `[dev-dependencies]` instead of `[dependencies]` |
| `-p <name>` / `--package <name>` | Workspace member to edit |

Installs always include development dependencies. See [Project settings](project-settings.md#dependency-keys) for the matching `dream.toml` keys.

## The lockfile: `dream.lock`

`dreamer install` (and every command that implies it) writes `dream.lock`: the exact, pinned
dependency versions. It should be committed to version control for applications so every checkout
resolves to the same dependency versions.

```toml
version = 2

[[package]]
name = "json-tools"
version = "0.3.1"
source = "registry+https://raw.githubusercontent.com/sps014/dream-registry/main"
checksum = "sha256:9f2c...ab31"
dependencies = []
```

| Field | Meaning |
|---|---|
| `version` (top level) | Lockfile format; the current format is `2`, and older files must be regenerated with `dreamer update` |
| `name` | Package name as written in `dream.toml` |
| `version` | Exact resolved version |
| `source` | Where the package came from (see below) |
| `checksum` | `sha256:` checksum of a registry package |
| `git_selector` | The tag, branch or revision a Git dependency asked for (`HEAD` when none) |
| `dependencies` | Resolved dependencies, each as a `"name version"` string |

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

See [Imports & Modules](../language/rules/imports.md) for the base (non-package) import syntax.

The LSP suggests installed package names when you type `import `, reading from `dream_packages/` —
no separate configuration needed.


## Dependency resolution

Registry dependencies resolve to the highest version that satisfies every accumulated requirement.
`path` and `git` dependencies are pinned from their own `dream.toml` and are never subject to
registry version selection. Conflicting requirements produce a clear error naming both sides.

---
title: Publishing
summary: Share reusable Dream source through a package registry.
icon: material/package-up
---

# Publish a source package

Publishing shares reusable Dream source so other projects can install it with `dreamer add`. To share a finished app, use [packaging](index.md).

## Prepare the package

Check the name, version, description, license, and dependencies in `dream.toml`, run the tests, and include a README that explains what the package does and how to use it.

```sh
dreamer test
dreamer publish
```

Publishing uploads the package, so run it only when you intend to share that version, and keep the source archive at or under 10 MiB. Path-only dependencies need a `version` as well, because another machine cannot use your local folders, and test packages (`type = "test"`) are never published.

| Archive entry | Included when |
|---|---|
| `dream.toml` | Always |
| `src/` | The folder exists |
| `native/` | The folder exists |
| README | The first of `README.md`, `README`, `Readme.md`, `readme.md` that exists |

The registry entry records the name, version, `[dependencies]` version requirements and checksum, plus `description`, `authors`, `license`, `edition`, `type`, `targets` and `keywords` from `[package]`. See [Registries](registries.md#registry-protocol) for the entry format.

## Choose a registry

| Option | Effect |
|---|---|
| `--registry <url>` | Registry base URL (default: `[registries] default`, then the public registry) |
| `--token <token>` | Publish token for GitHub-hosted registries |
| `-p <name>` / `--package <name>` | Workspace member to publish |

Without `--token`, the token comes from `DREAM_REGISTRY_TOKEN`, then `GITHUB_TOKEN`. Keep tokens out of source files and version control.

In a workspace, use `-p name` to choose the package. See [Workspaces](../tooling/workspaces.md) for publishing related packages and [Registries](registries.md) for private or local registries.

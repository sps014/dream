---
title: Publishing
summary: Share reusable Dream source through a package registry.
icon: material/package-up
---

# Publish a source package

Publishing shares reusable Dream source through a package registry. Other projects can then install it with `dreamer add`. To share a finished app instead, use [packaging](index.md).

## Prepare the package

Check its name, version, description, license, and dependencies in `dream.toml`. Run its tests. Include a README explaining what it does and how to use it.

```sh
dreamer test
dreamer publish
```

Publishing uploads your package; run it only when you intend to share that version. The source archive must be at most 10 MiB. Path-only dependencies cannot be published because another machine cannot resolve your local folders; give them a `version` as well. Test packages (`type = "test"`) are never published.

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

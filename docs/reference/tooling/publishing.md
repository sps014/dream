# Publish a source package

Publishing shares reusable Dream source through a package registry. Other projects can then install it with `dreamer add`. To share a finished app instead, use [packaging](pack/index.md).

## Prepare the package

Check its name, version, description, license, and dependencies in `dream.toml`. Run its tests. Include a README explaining what it does and how to use it.

```sh
dreamer test
dreamer publish
```

Publishing uploads your package; run it only when you intend to share that version. The source archive includes `dream.toml` and `src/`, and must be at most 10 MiB. Path-only dependencies cannot be published because another machine cannot resolve your local folders.

## Choose a registry

Use `--registry <url>` to select a registry. Authentication can come from `DREAM_REGISTRY_TOKEN` or `--token`. Keep tokens out of source files and version control.

In a workspace, use `-p name` to choose the package. See [Workspaces](workspaces.md) for publishing related packages and [Registries](registries.md) for private or local registries.

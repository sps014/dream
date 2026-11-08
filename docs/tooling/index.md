---
title: Tooling
summary: "Create, build, test, and manage projects with `dreamer`, plus editor and formatter setup."
icon: material/tools
hide: [toc]
---

# Dreamer: manage your project

`dreamer` creates, runs, tests, and shares Dream projects. Run it from the project folder: it reads `dream.toml` and installs the packages the project needs.

```sh
dreamer init console hello
cd hello
dreamer run
```

## Guides in this section

<!-- pages -->

To share a finished app or publish a package, see [Deploy](../deploy/index.md).

## Install Dreamer

The [Dream installer](../learn/quickstart.md) includes `dreamer`. Open a new terminal after installing it, then run `dreamer --help`.

If a command fails, start with [Troubleshooting](../learn/troubleshooting.md). Editor setup is covered in [Editor support](editor.md).

---
title: Deploy
summary: "Package apps for desktop, browser, Node, and mobile, and publish packages."
icon: material/rocket-launch-outline
hide: [navigation, toc]
---

# Package and share an app

Packaging puts a finished app and the files it needs into a distributable layout. Publishing a package instead shares its Dream source for use by other projects.

## Guides in this section

<!-- cards -->

## Start with your own computer

Run these commands from a runnable project:

```sh
dreamer run --release
dreamer pack
```

`dreamer pack` creates CLI output under `target/pack/`. It does not upload your app. See [Packaging options and troubleshooting](options.md) before building for another computer.

For reusable source packages, use [Publishing](publishing.md).

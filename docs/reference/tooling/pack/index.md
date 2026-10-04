# Package and share an app

Packaging puts a finished app and the files it needs into a distributable layout. Publishing a package instead shares its Dream source for use by other projects.

| Where your app will run | Guide |
| --- | --- |
| Windows, macOS, or Linux | [Desktop packaging](desktop.md) |
| A browser or Node | [Web and Node output](web-node.md) |
| Inside an iOS or Android app | [Mobile libraries](mobile.md) |

## Start with your own computer

Run these commands from a runnable project:

```sh
dreamer run --release
dreamer pack
```

`dreamer pack` creates desktop output under `target/pack/`. It does not upload your app. See [Packaging options and troubleshooting](options.md) before building for another computer.

For reusable source packages, use [Publishing](../publishing.md).

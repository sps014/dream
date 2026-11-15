---
title: Browser and Node
summary: Ship WebAssembly output for the browser or Node.
icon: material/web-box
---

# Share browser and Node output

Use this guide after your app runs with `dreamer run --target web` or `dreamer run --target node`.

Native `dreamer pack` creates native executables. For a browser or Node, list the hosts in `dream.toml`, then build:

```toml
[package]
targets = ["web", "node"]
```

```sh
dreamer build --release
```

`dreamer build` has no `--target` flag: it produces browser and Node output for whichever of `web` and `node` are listed in `[package].targets`. `dreamer run --target web` or `--target node` builds only the selected host before running it.

| Folder | Files | Used by |
|---|---|---|
| `target/web/` | `<stem>.wasm`, `<stem>.wat`, `<stem>.abi.json`, `<stem>.web.runtime.js` | The browser page (`index.html`) |
| `target/node/` | `<stem>.wasm`, `<stem>.abi.json`, `<stem>.node.runtime.js`, your assets | `run.mjs` under Node |

`<stem>` is the entry file's name without `.dream`, such as `main`, and debug and release builds share these folders. Optimized builds also write `.gz` and `.br` copies of the `.wasm` and runtime files in `target/web/` for servers that serve pre-compressed files.

## Browser

Keep your page, assets, and the generated files under `target/web/` together. `[package].assets` are copied into `target/web/` and `target/node/` on every build, and the page from `dreamer init web` loads the program from that folder.

Serve the files through an HTTP server. Opening the page directly from disk can block loading.

Use `dreamer run --target web` to check the page locally. Deploy its files through your chosen web host when you are ready.

## Node

Keep `run.mjs`, the generated files under `target/node/`, and any assets your app reads. The person running it needs Node, and starts the scaffolded launcher with `node run.mjs`.

## Check before sharing

Use the same release output you tested, and check asset paths, permissions, and environment variables on a clean machine. [Build and run](../tooling/build-run.md) covers environment selection, and [integration](../language/interop/index.md) covers loading and calling the program.

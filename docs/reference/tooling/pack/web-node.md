# Share browser and Node output

Use this guide after your app runs with `dreamer run --target web` or `dreamer run --target node`.

Desktop `dreamer pack` creates native executables. For a browser or Node, build the corresponding output instead:

```sh
dreamer build --release --target web
dreamer build --release --target node
```

## Browser

Keep your page, assets, and the generated files under `target/web/` together. The page created by `dreamer init --runtime web` loads the generated program from that folder. Serve the files through an HTTP server; opening the page directly from disk can block loading.

Use `dreamer run --target web` to check the page locally. Deploy its files through your chosen web host when you are ready.

## Node

Keep `run.mjs`, the generated files under `target/node/`, and any assets your app reads. The person running it needs Node. Run the scaffolded launcher with `node run.mjs`.

## Check before sharing

Use the same release output you tested. Check relative asset paths, permissions, and environment variables on a clean machine. [Build and run](../build-run.md) explains environment selection; [integration](../../language/interop.md) explains loading and calling your program.

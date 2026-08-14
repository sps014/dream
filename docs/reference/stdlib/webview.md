# WebView

**Import:** `import system.webview;` — **native only** (`dream run`). Browser and Node report unsupported.

Opens a desktop window (via wry) and talks to the page with typed JSON IPC. Do not mix with [`GpuSurface`](gpu.md) in the same process.

```dream
import system;
import system.webview;

async fun main(): void {
    switch (WebView.create("Hello", 1024, 768)) {
        Ok(view) => {
            view.load_url("https://example.com");
            await view.run();
        },
        Err(e) => System.println(e.message()),
    }
}
```

| Call | Meaning |
| --- | --- |
| `WebView.create(title, width, height)` | open a window |
| `load_url` / `load_html` / `load_file` | set the document |
| `await run()` | event loop until closed |
| `close()` | close the window |
| `await eval(js)` | run JavaScript, get a string |

Typed IPC uses `@json` types and `window.Dream` on the page (`on` / `serve` / `emit`). There is also a raw bytes path.

Samples: [`hello.dream`](https://github.com/sps014/dream/tree/main/sample/webview/hello.dream), [`ipc.dream`](https://github.com/sps014/dream/tree/main/sample/webview/ipc.dream).

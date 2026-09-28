# WebView

**Import:** `import system.webview;` — **native only** (`dream run`). Browser and Node report unsupported.

The public installer ships WebView on every OS. On Linux it installs WebKitGTK/GTK if those libraries are missing. A desktop display is required to open a window (`DISPLAY` / Wayland); headless Docker needs Xvfb or `-e DISPLAY`.

Opens a desktop window and talks to the page with typed JSON IPC. Do not mix with [`GpuSurface`](gpu.md) in the same process.

```dream
import system;
import system.webview;

async fun main(): void {
    switch (WebView.create("Hello", 1024, 768)) {
        Ok(view) => {
            view.load_url("https://example.com");
            view.run().await;
        },
        Err(e) => System.println(e.message()),
    }
}
```

| Call | Meaning |
| --- | --- |
| `WebView.create(title, width, height)` | open a window |
| `load_url` / `load_html` / `load_file` | set the document |
| `run().await` | event loop until closed; optional `token` |
| `close()` | close the window |
| `close_requested()` | true after the user asked to close |
| `eval(js).await` | run JavaScript, get a string; optional `token` |

## Typed IPC

`on<T>` / `serve<Req, Res>` / `emit<T>` carry JSON between Dream and `window.Dream` on the page. Incoming messages are `@json` classes; `emit` also accepts `string`, `int`, `bool` and `double`.

```dream
@json
class Ping {
    public text: string;
    public constructor(text: string) { this.text = text; }
}

view.on<Ping>("ping", (msg) => System.println(msg.text));
view.emit("ready", "ok");
```

```js
Dream.emit("ping", JSON.stringify({ text: "hi" }));
Dream.on("ready", (v) => console.log(v));
```

## Raw bytes

The byte calls skip JSON and base64 entirely. Bodies cross as-is over the `dream-ipc` URL scheme (`fetch` on the page), so multi-megabyte payloads are cheap.

| Dream | Page | Direction |
| --- | --- | --- |
| `serve_bytes(ch, (b) => out)` | `Dream.invokeBytes(ch, u8)` → `Promise<Uint8Array>` | request/response, handler runs inline |
| `serve_bytes_async(ch, async (b) => out)` | same | handler may `await` (HTTP, files, timers) |
| `on_bytes(ch, (b) => ...)` | `Dream.emitBytes(ch, u8)` | page → Dream |
| `emit_bytes(ch, bytes)` | `Dream.onBytes(ch, (u8) => ...)` | Dream → page |

An `invokeBytes` for a channel with no handler rejects with `no handler for channel: <ch>`.

`serve_bytes_async` runs its handler on the same scheduler as `run()`, so the window keeps pumping while the reply is pending. Use it to fetch on demand instead of preloading everything before `run()`:

```dream
view.serve_bytes_async("track", async (body) => {
    let id = int.parse(Encoding.utf8_decode(body)).unwrap_or(0);
    switch (HttpClient().get(url_for(id)).await) {
        Ok(r) => return r.bytes(),
        Err(_) => return Buffer.alloc<byte>(0),
    }
});
```

Handlers often capture `view` itself. `close()`, or the user closing the window, drops every registered handler so those captures are freed.

Samples: [`hello.dream`](https://github.com/sps014/dream/tree/main/sample/webview/hello.dream), [`ipc.dream`](https://github.com/sps014/dream/tree/main/sample/webview/ipc.dream), [`bytes.dream`](https://github.com/sps014/dream/tree/main/sample/webview/bytes.dream).

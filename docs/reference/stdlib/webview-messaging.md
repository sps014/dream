# Send messages to a page

Use named channels to pass data between your Dream app and the page in its window.

[Back to overview](webview.md)

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

Samples: [`hello.dream`](https://github.com/sps014/dream/tree/main/sample/webview/hello.dream), [`ipc.dream`](https://github.com/sps014/dream/tree/main/sample/webview/ipc.dream), [`bytes.dream`](https://github.com/sps014/dream/tree/main/sample/webview/bytes.dream), [`desktop.dream`](https://github.com/sps014/dream/tree/main/sample/webview/desktop.dream).

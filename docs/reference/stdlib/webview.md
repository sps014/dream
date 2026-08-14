# `system.webview`

Native desktop WebView windows via [wry](https://github.com/tauri-apps/wry), with typed IPC on the Dream side.

**Package:** `import system.webview;` — **`@native` only** (`dream run`). Browser/Node hosts report unsupported.

Do not mix with [`system.gpu`](gpu.md) `GpuSurface` in the same process (separate winit event loops).

Samples:

| Sample | Role |
|--------|------|
| [`sample/webview/hello.dream`](https://github.com/sps014/dream/tree/main/sample/webview/hello.dream) | Open a URL |
| [`sample/webview/ipc.dream`](https://github.com/sps014/dream/tree/main/sample/webview/ipc.dream) | Typed `on` / `serve` / `emit` |

## Create and load

#### `WebView.create(title, width, height): Result<WebView, WebViewError>`

Opens a native window.

#### `load_url(url)` / `load_html(html)` / `load_file(path)`

Navigate or set document content. `load_file` uses a `file://` URL so relative assets beside the HTML file resolve.

```dream
switch (WebView.create("Hello", 1024, 768)) {
    Ok(view) => {
        view.load_url("https://example.com");
        await view.run();
    },
    Err(e) => System.println(e.message()),
}
```

## Typed IPC

Payloads are `@json` types. The page talks JSON through the injected `window.Dream` bridge.

| Dream | Page JS |
|-------|---------|
| `view.on<T>("ch", handler)` | `Dream.emit("ch", JSON.stringify(...))` |
| `view.serve<Req, Res>("ch", handler)` | `await Dream.invoke("ch", JSON.stringify(...))` |
| `view.emit("ch", value)` / `WebView.emit_on(id, …)` | `Dream.on("ch", (body) => { ... })` |

Prefer `WebView.emit_on(id, …)` inside `on`/`serve` handlers so you capture the window `id` instead of the `WebView` instance.

```dream
@json
class Inc {
    public by: int;
    public constructor(by: int) { this.by = by; }
}

@json
class Count {
    public value: int;
    public constructor(value: int) { this.value = value; }
}

@json
class Empty {
    public constructor() {}
}

class CounterState {
    public count: int;
    public constructor() { this.count = 0; }
}

async fun main() {
    let state = CounterState();
    switch (WebView.create("Counter", 480, 320)) {
        Ok(view) => {
            let id = view.id;
            view.on<Inc>("inc", (msg) => {
                state.count = state.count + msg.by;
                WebView.emit_on(id, "count", Count(state.count));
            });
            view.serve<Empty, Count>("get_count", (_req) => Count(state.count));

            view.load_file("ui/index.html"); // or sample/webview/ui/index.html from repo root

            await view.run();
        },
        Err(e) => System.println(e.message()),
    }
}
```

```js
Dream.emit("inc", JSON.stringify({ by: 1 }));
const c = JSON.parse(await Dream.invoke("get_count", "{}"));
Dream.on("count", (body) => { /* ... */ });
```

No-arg commands use an empty `@json` type (e.g. `Empty`).

## Raw IPC (bytes / unmanaged arrays)

For hot paths, skip JSON and move opaque `byte[]` (or packed unmanaged arrays). On the page, use `Uint8Array`.

| Dream | Page JS |
|-------|---------|
| `view.on_bytes("ch", handler)` | `Dream.emitBytes("ch", uint8Array)` |
| `view.serve_bytes("ch", handler)` | `await Dream.invokeBytes("ch", uint8Array)` |
| `view.emit_bytes("ch", data)` / `WebView.emit_bytes_on(id, …)` | `Dream.onBytes("ch", (u8) => { … })` |
| `view.on_raw<T>("ch", handler)` | same as `emitBytes` (packed `T[]` bytes) |
| `view.serve_raw<TIn, TOut>(…)` | same as `invokeBytes` |
| `view.emit_raw<T>("ch", values)` / `emit_raw_on` | `onBytes` receives packed element bytes |

`T` / `TIn` / `TOut` must be `unmanaged` (blittable). Arrays are packed contiguously (`Bytes.of` per element), same idea as GPU buffer uploads.

```dream
view.on_bytes("frame", (data) => {
    // data: byte[]
});

view.serve_bytes("echo", (data) => data);

view.emit_raw<float>("samples", samples); // samples: float[]
```

```js
Dream.emitBytes("frame", new Uint8Array([1, 2, 3]));
const out = await Dream.invokeBytes("echo", new Uint8Array([9]));
Dream.onBytes("samples", (u8) => { /* Float32 view over u8.buffer */ });
```

Host↔page still crosses wry's string IPC as base64; Dream keeps the payload as raw `byte[]` (no JSON parse).

## Lifecycle

#### `await run()`

Pumps the window and dispatches IPC until the user closes it.

#### `close()`

Close early from Dream.

#### `await eval(js): Result<string, WebViewError>`

Run page script; returns a stringified completion value when available.

## Errors

`WebViewError` implements [`Error`](option-result.md) with `message()` / `code()` (`UNAVAILABLE`, `EFAILED`, `EUNSUPPORTED`, `EPARSE`).

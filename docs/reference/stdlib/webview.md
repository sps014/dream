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
| `eval(js).await` | run a JavaScript function body (`return` a value), get a string; optional `token` |

## Window

Window state is exposed as properties. Getters read the live window, and on a closed view they return `0` / `false` / `""` while setters do nothing. Sizes and positions are logical pixels.

| Property | Type | Notes |
| --- | --- | --- |
| `title` | `string` | |
| `icon` | `string` | set a PNG path to change the icon; `""` means the one compiled in from `[package].icon` |
| `width`, `height` | `int` | inner size |
| `min_width`, `min_height`, `max_width`, `max_height` | `int` | `0` means no limit |
| `x`, `y` | `int` | outer position |
| `resizable`, `fullscreen`, `maximized`, `minimized`, `always_on_top`, `visible` | `bool` | |
| `devtools_open` | `bool` | the web inspector |
| `page_dialogs` | `bool` | lets page JS open native dialogs, see [Page dialogs](#page-dialogs) |
| `focused`, `scale_factor`, `dark_mode` | `bool` / `float` / `bool` | read-only |
| `window_id` | `int` | pass as `parent` to [desktop dialogs](desktop.md) |

`resize(width, height)`, `center()`, `focus()` and `restore()` (leave fullscreen, minimized and maximized) are methods.

```dream
view.title = "Player";
view.min_width = 480;
view.min_height = 320;
view.center();
view.devtools_open = true;
```

The icon for every window and the macOS Dock comes from `[package].icon` in `dream.toml` (see [dreamer](../tooling/dreamer.md)).

## Window events

| Handler | Called with |
| --- | --- |
| `on_resize((w, h) => ...)` | new inner size |
| `on_move((x, y) => ...)` | new position |
| `on_focus((focused) => ...)` | `bool` |
| `on_minimize((minimized) => ...)` / `on_maximize((maximized) => ...)` | `bool` |
| `on_scale_change((scale) => ...)` | `float`, e.g. moving to a Retina screen |
| `on_theme_change((dark) => ...)` | `bool`, the OS switched light/dark |
| `on_close_requested(() => bool)` | the user clicked close: return `false` to keep the window open |

Without an `on_close_requested` handler the window closes as soon as the user asks. Events fire for user actions and for changes made from Dream (`view.width = 800` fires `on_resize`), and a burst of resizes while dragging arrives as the latest size only.

```dream
view.on_resize((w, h) => System.println("size " + w + "x" + h));
view.on_close_requested(() => !has_unsaved_changes);
```

The page gets the same events: `Dream.onWindowEvent((e) => ...)` receives `{ type: "resized", width, height }`, `{ type: "focused", focused }`, `{ type: "moved", x, y }`, `{ type: "minimized", minimized }`, `{ type: "maximized", maximized }`, `{ type: "scale", scale }`, `{ type: "theme", dark }` and `{ type: "close_requested" }`.

## File drop

Files dragged onto the window arrive as absolute paths, in Dream and on the page:

```dream
view.on_file_drop((paths) => System.println("dropped " + paths.length));
```

```js
Dream.onFileDrop((paths) => console.log(paths));
```

## Page dialogs

After `view.page_dialogs = true`, page JS can open native dialogs attached to the window. The flag is off by default so a remote page loaded with `load_url` cannot open them; while it is off, every call rejects.

```js
const path = await Dream.dialog.openFile({
  title: "Open image",
  filters: [{ name: "Images", extensions: ["png", "jpg"] }],
});
const paths = await Dream.dialog.openFiles({ directory: "/tmp" });
const folder = await Dream.dialog.openFolder({});
const folders = await Dream.dialog.openFolders({});
const target = await Dream.dialog.saveFile({ fileName: "notes.txt" });
const choice = await Dream.dialog.message({ title: "Delete?", text: "This cannot be undone.", level: "warning", buttons: "yesNo" });
```

Pickers resolve to a path, or an array for `openFiles` / `openFolders`, and to `null` when cancelled. `message` resolves to `"ok"`, `"cancel"`, `"yes"` or `"no"`; `level` is `"info"`, `"warning"` or `"error"`, and `buttons` is `"ok"`, `"okCancel"`, `"yesNo"` or `"yesNoCancel"`. Dream-side dialogs, the clipboard and `Shell.open` live in [`system.desktop`](desktop.md).

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

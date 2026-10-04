# WebView


A WebView puts a web page inside a desktop window. Use it for an app with a page-based interface and send messages between the page and your Dream code. It requires a desktop display and is not available in browser or Node programs.

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

## Explore this topic

- [Respond to window events](webview-events.md)
- [Send messages to a page](webview-messaging.md)

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
| `page_dialogs` | `bool` | lets page JS open native dialogs, see [Page dialogs](webview-events.md#page-dialogs) |
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

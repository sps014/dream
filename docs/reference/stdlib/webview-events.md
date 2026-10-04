# Respond to window events

React to changes in your desktop window, file drops, and page dialogs. Start with the window setup guide first.

[Back to overview](webview.md)

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

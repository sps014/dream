# Desktop


Use these desktop helpers to open dialogs, read or write the clipboard, and open a URL or file in its usual application. They can work on their own or alongside a WebView window.

**Import:** `import system.desktop;` — **native only** (`dream run`). Browser and Node report unsupported: dialogs read as cancelled, the clipboard as empty, and `Shell.open` returns an error.

Native file and message dialogs, the system clipboard, and opening URLs or files in their default app. Works on its own or next to a [WebView](webview.md).

```dream
import system;
import system.desktop;

async fun main(): void {
    let dialog = FileDialog();
    dialog.title = "Open a track";
    dialog.add_filter("Audio", ["mp3", "flac"]);
    switch (dialog.pick_file().await) {
        Some(path) => System.println("picked " + path),
        None => System.println("cancelled"),
    }
}
```

## Dialogs

Dialogs are `async`: other tasks, timers and an open WebView keep running while one is open. With a WebView window open, a dialog attaches to it as a sheet (macOS) or an owned window. On macOS, a dialog opened when the program has no window runs modally and pauses the Dream task loop until it closes.

### `FileDialog`

| Property | Meaning |
| --- | --- |
| `title` | window title |
| `directory` | folder the dialog starts in |
| `file_name` | suggested name for `save_file` |
| `parent` | `view.window_id` of the WebView to attach to (default: the app's main window) |
| `filters` | the `FileFilter`s added so far (read-only) |

| Call | Result |
| --- | --- |
| `add_filter(name, extensions)` | adds a filter, e.g. `("Images", ["png", "jpg"])` |
| `pick_file().await` | `Option<string>` |
| `pick_files().await` | `List<string>`, empty when cancelled |
| `pick_folder().await` | `Option<string>` |
| `pick_folders().await` | `List<string>`, empty when cancelled |
| `save_file().await` | `Option<string>` |

### `MessageDialog`

| Property | Values |
| --- | --- |
| `title`, `text` | strings |
| `level` | `MessageLevel.Info` (default), `Warning`, `Error` |
| `buttons` | `MessageButtons.Ok` (default), `OkCancel`, `YesNo`, `YesNoCancel` |
| `parent` | `view.window_id`, like `FileDialog` |

`show().await` returns the `MessageResult` the user picked: `Ok`, `Cancel`, `Yes` or `No`. Closing the dialog without a choice reads as `Cancel`.

```dream
let ask = MessageDialog();
ask.title = "Unsaved changes";
ask.text = "Save before closing?";
ask.level = MessageLevel.Warning;
ask.buttons = MessageButtons.YesNoCancel;
if ask.show().await == MessageResult.Yes {
    save();
}
```

Shortcuts: `Dialog.alert(text).await` shows an OK box, and `Dialog.confirm(text).await` returns `true` when the user picks Yes.

## Clipboard

Every clipboard member is static. Reads return empty (`""`, an empty array or list) when the clipboard holds nothing in that format.

| Member | Type | Notes |
| --- | --- | --- |
| `Clipboard.text` | `string` | get / set |
| `Clipboard.html` | `string` | setting also writes a plain-text copy for apps that can't paste HTML |
| `Clipboard.image` | `byte[]` | PNG bytes in and out |
| `Clipboard.files` | `List<string>` | file paths, as copied in Finder / Explorer / the file manager |
| `Clipboard.formats` | `List<string>` | formats present right now (MIME names or platform type ids), read-only |
| `Clipboard.set_data(format, bytes)` | | raw bytes under a custom format, e.g. `application/x-myapp-track` |
| `Clipboard.data(format)` | `Option<byte[]>` | `None` when the format is absent |
| `Clipboard.has(format)` | `bool` | |
| `Clipboard.clear()` | | |

```dream
Clipboard.text = "copied from Dream";
let png = Clipboard.image;
if png.length > 0 {
    File.write_bytes("paste.png", png).await;
}
```

## Opening URLs and files

`Shell.open(target)` opens a URL in the default browser, or a file or folder in its default app. It returns `Err(DesktopError)` when the system could not start an app.

```dream
Shell.open("https://example.com").unwrap();
```

Sample: [`desktop.dream`](https://github.com/sps014/dream/tree/main/sample/webview/desktop.dream).

# WebView

**Import:** `import system.webview;`

Read the [usage guide](../stdlib/webview.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](webview-webview.md)

## `class WebView`

```dream
public class WebView
```

## `window_id`

Pass to `FileDialog.parent` / `MessageDialog.parent` to attach a dialog to this window.

```dream
public get window_id(): int
```

## `title`

```dream
public get title(): string
```

## `title`

```dream
public set title(value: string)
```

## `icon`

PNG path of the runtime icon; `""` while the compiled-in `[package].icon` is in use. An unreadable file is reported on stderr and the current icon stays.

```dream
public get icon(): string
```

## `icon`

```dream
public set icon(path: string)
```

## `width`

Inner size in logical pixels.

```dream
public get width(): int
```

## `width`

```dream
public set width(value: int)
```

## `height`

```dream
public get height(): int
```

## `height`

```dream
public set height(value: int)
```

## `min_width`

Size limits in logical pixels; `0` means no limit.

```dream
public get min_width(): int
```

## `min_width`

```dream
public set min_width(value: int)
```

## `min_height`

```dream
public get min_height(): int
```

## `min_height`

```dream
public set min_height(value: int)
```

## `max_width`

```dream
public get max_width(): int
```

## `max_width`

```dream
public set max_width(value: int)
```

## `max_height`

```dream
public get max_height(): int
```

## `max_height`

```dream
public set max_height(value: int)
```

## `x`

Outer position of the window on the desktop, in logical pixels.

```dream
public get x(): int
```

## `x`

```dream
public set x(value: int)
```

## `y`

```dream
public get y(): int
```

## `y`

```dream
public set y(value: int)
```

## `resizable`

```dream
public get resizable(): bool
```

## `resizable`

```dream
public set resizable(value: bool)
```

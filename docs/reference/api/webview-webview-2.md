# WebView

**Import:** `import system.webview;`

Read the [usage guide](../stdlib/webview.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](webview-webview.md)

## `fullscreen`

Borderless fullscreen on the current monitor.

```dream
public get fullscreen(): bool
```

## `fullscreen`

```dream
public set fullscreen(value: bool)
```

## `maximized`

```dream
public get maximized(): bool
```

## `maximized`

```dream
public set maximized(value: bool)
```

## `minimized`

```dream
public get minimized(): bool
```

## `minimized`

```dream
public set minimized(value: bool)
```

## `always_on_top`

```dream
public get always_on_top(): bool
```

## `always_on_top`

```dream
public set always_on_top(value: bool)
```

## `visible`

```dream
public get visible(): bool
```

## `visible`

```dream
public set visible(value: bool)
```

## `devtools_open`

The web inspector for this page.

```dream
public get devtools_open(): bool
```

## `devtools_open`

```dream
public set devtools_open(value: bool)
```

## `page_dialogs`

Lets page JS open native dialogs through `Dream.dialog.*`. Off by default, so remote pages loaded with `load_url` cannot open dialogs unless you opt in.

```dream
public get page_dialogs(): bool
```

## `page_dialogs`

```dream
public set page_dialogs(value: bool)
```

## `focused`

```dream
public get focused(): bool
```

## `scale_factor`

Physical pixels per logical pixel (2.0 on most Retina displays).

```dream
public get scale_factor(): float
```

## `dark_mode`

```dream
public get dark_mode(): bool
```

## `resize`

Sets width and height together (one relayout).

```dream
public fun resize(width: int, height: int): void
```

## `center`

Centers the window on its current monitor.

```dream
public fun center(): void
```

## `focus`

```dream
public fun focus(): void
```

## `restore`

Leaves fullscreen, minimized and maximized states.

```dream
public fun restore(): void
```

## `create`

```dream
public static fun create(title: string, width: int, height: int): Result<WebView, WebViewError>
```

## `load_url`

```dream
public fun load_url(url: string): Result<bool, WebViewError>
```

## `load_html`

```dream
public fun load_html(html: string): Result<bool, WebViewError>
```

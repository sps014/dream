# WebView

**Import:** `import system.webview;`

Read the [usage guide](../stdlib/webview.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](webview-webview.md)

## `on_close_requested`

Asked when the user closes the window; any handler returning `false` keeps it open. Without a handler the window closes straight away.

```dream
public fun on_close_requested(handler: fun(): bool): void
```

## `eval`

```dream
public async fun eval(js: string, token: Option<CancellationToken> = Option.None): Result<string, WebViewError>
```

# WebView

**Import:** `import system.webview;`

Read the [usage guide](../stdlib/webview.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](webview-webview.md)

## `load_file`

```dream
public fun load_file(path: string): Result<bool, WebViewError>
```

## `on`

```dream
public fun on<T>(channel: string, handler: fun(T): void): void
```

## `serve`

```dream
public fun serve<Req, Res>(channel: string, handler: fun(Req): Res): void
```

## `emit`

```dream
public fun emit<T>(channel: string, value: T): void
```

## `emit_on`

```dream
public static fun emit_on<T>(id: int, channel: string, value: T): void
```

## `on_bytes`

```dream
public fun on_bytes(channel: string, handler: fun(byte[]): void): void
```

## `serve_bytes`

```dream
public fun serve_bytes(channel: string, handler: fun(byte[]): byte[]): void
```

## `serve_bytes_async`

Like `serve_bytes`, but the handler may await (HTTP, files, timers). It runs on the same task scheduler as `run()`, so the window keeps pumping while the reply is pending.

```dream
public fun serve_bytes_async(channel: string, handler: fun(byte[]): Future<byte[]>): void
```

## `emit_bytes`

```dream
public fun emit_bytes(channel: string, data: byte[]): void
```

## `emit_bytes_on`

```dream
public static fun emit_bytes_on(id: int, channel: string, data: byte[]): void
```

## `on_raw`

```dream
public fun on_raw<T : unmanaged>(channel: string, handler: fun(T[]): void): void
```

## `serve_raw`

```dream
public fun serve_raw<TIn : unmanaged, TOut : unmanaged>( channel: string, handler: fun(TIn[]): TOut[] ): void
```

## `emit_raw`

```dream
public fun emit_raw<T : unmanaged>(channel: string, values: T[]): void
```

## `emit_raw_on`

```dream
public static fun emit_raw_on<T : unmanaged>(id: int, channel: string, values: T[]): void
```

## `run`

```dream
public async fun run(token: Option<CancellationToken> = Option.None): void
```

## `close`

```dream
public fun close(): void
```

## `on_file_drop`

Files dropped onto the window, as absolute paths.

```dream
public fun on_file_drop(handler: fun(List<string>): void): void
```

## `on_resize`

```dream
public fun on_resize(handler: fun(int, int): void): void
```

## `on_move`

```dream
public fun on_move(handler: fun(int, int): void): void
```

## `on_focus`

```dream
public fun on_focus(handler: fun(bool): void): void
```

## `on_minimize`

```dream
public fun on_minimize(handler: fun(bool): void): void
```

## `on_maximize`

```dream
public fun on_maximize(handler: fun(bool): void): void
```

## `on_theme_change`

```dream
public fun on_theme_change(handler: fun(bool): void): void
```

## `on_scale_change`

```dream
public fun on_scale_change(handler: fun(float): void): void
```

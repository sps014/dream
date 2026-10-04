# WebViewError

**Import:** `import system.webview;`

Read the [usage guide](../stdlib/webview.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class WebViewError : Error`

Failure from native WebView create / load / eval / IPC.

```dream
public class WebViewError : Error
```

## `constructor`

```dream
public constructor(code: string, message: string)
```

## `message`

```dream
public fun message(): string
```

## `code`

```dream
public fun code(): string
```

## `unavailable`

```dream
public static fun unavailable(message: string): WebViewError
```

## `failed`

```dream
public static fun failed(message: string): WebViewError
```

## `unsupported`

```dream
public static fun unsupported(message: string): WebViewError
```

## `parse`

```dream
public static fun parse(message: string): WebViewError
```

## `cancelled`

Cooperative cancellation (`CancellationToken`).

```dream
public static fun cancelled(): WebViewError
```

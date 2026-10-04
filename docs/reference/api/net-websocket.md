# WebSocket

**Import:** `import system.net;`

Read the [usage guide](../stdlib/http.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class WebSocket`

A WebSocket client. Native uses tungstenite (`ws://` and `wss://`). The browser/Node hosts use the standard `WebSocket` API.

```dream
public class WebSocket
```

## `connect`

Opens a WebSocket connection to `url` with no connect timeout.

```dream
public static async fun connect(url: string, token: Option<CancellationToken> = Option.None): Result<WebSocket, NetError>
```

## `connect_timeout`

Opens a WebSocket connection with a connect timeout in milliseconds (`0` = none).

```dream
public static async fun connect_timeout(url: string, timeout_ms: int, token: Option<CancellationToken> = Option.None): Result<WebSocket, NetError>
```

## `send_text`

Sends a text frame. `Ok(true)` on success.

```dream
public async fun send_text(text: string, token: Option<CancellationToken> = Option.None): Result<bool, NetError>
```

## `send_binary`

Sends a binary frame. `Ok(true)` on success.

```dream
public async fun send_binary(data: byte[], token: Option<CancellationToken> = Option.None): Result<bool, NetError>
```

## `receive`

Waits for the next message. Resolves `NetError` on transport failure; a peer-initiated close arrives as `Ok(WebSocketMessage.Close(...))`, not an error.

```dream
public async fun receive(token: Option<CancellationToken> = Option.None): Result<WebSocketMessage, NetError>
```

## `close`

Closes the connection with an optional close code (default `1000`) and reason. Safe to call more than once.

```dream
public fun close(): void
```

## `close_with`

Closes the connection with an explicit close code/reason.

```dream
public fun close_with(code: int, reason: string): void
```

# Websocket

**Import:** `import system.net;`

## `WebSocket`

An owned RFC 6455 WebSocket, available on native, Node and browser hosts.

```dream
public class WebSocket
```

## `connect`

Opens ws:// or wss:// with an optional deadline and inherited cancellation.

```dream
public static async fun connect(
    url: string,
    timeout_ms: int = 0,
    token: Option<CancellationToken> = Option.None
): Result<WebSocket, IoError>
```

## `send_text`

Sends one UTF-8 text message.

```dream
public fun send_text(
    text: string,
    token: Option<CancellationToken> = Option.None
): Future<Result<bool, IoError>>
```

## `send_binary`

Sends one binary message.

```dream
public fun send_binary(
    bytes: byte[],
    token: Option<CancellationToken> = Option.None
): Future<Result<bool, IoError>>
```

## `close`

Sends a normal closing handshake.

```dream
public fun close(token: Option<CancellationToken> = Option.None): Future<Result<bool, IoError>>
```

## `close_with`

Sends a validated close code and UTF-8 reason.

```dream
public fun close_with(
    code: int,
    reason: string,
    token: Option<CancellationToken> = Option.None
): Future<Result<bool, IoError>>
```

## `receive`

Receives one complete message. Only one receive may be pending at a time.

```dream
public fun receive(
    token: Option<CancellationToken> = Option.None
): Future<Result<WebSocketMessage, IoError>>
```

## `messages`

Iterates until Close, including the final closing message. Transport errors end iteration.

```dream
public fun messages(
    token: Option<CancellationToken> = Option.None
): AsyncIterable<WebSocketMessage>
```

# Server Ws

**Import:** `import system.webapi;`

For behavior and native target requirements, see [Native Web APIs](../../web-server/index.md).

## `ServerWebSocket`

A WebSocket accepted by a native WebApp route.

```dream
public class ServerWebSocket
```

## `send_text`

Sends one text message.

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

## `receive`

Receives one complete message.

```dream
public fun receive(
    token: Option<CancellationToken> = Option.None
): Future<Result<WebSocketMessage, IoError>>
```

## `messages`

Iterates messages through the final Close message.

```dream
public fun messages(
    token: Option<CancellationToken> = Option.None
): AsyncIterable<WebSocketMessage>
```

## `close`

Sends a normal closing handshake.

```dream
public fun close(token: Option<CancellationToken> = Option.None): Future<Result<bool, IoError>>
```

## `close_with`

Sends a closing handshake with a code and reason.

```dream
public fun close_with(
    code: int,
    reason: string,
    token: Option<CancellationToken> = Option.None
): Future<Result<bool, IoError>>
```

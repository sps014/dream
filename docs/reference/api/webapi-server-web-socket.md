# ServerWebSocket

**Import:** `import system.webapi;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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
): Future<Result<Unit, IoError>>
```

## `send_binary`

Sends one binary message.

```dream
public fun send_binary(
    bytes: byte[],
    token: Option<CancellationToken> = Option.None
): Future<Result<Unit, IoError>>
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
public fun close(token: Option<CancellationToken> = Option.None): Future<Result<Unit, IoError>>
```

## `close_with`

Sends a closing handshake with a code and reason.

```dream
public fun close_with(
    code: int,
    reason: string,
    token: Option<CancellationToken> = Option.None
): Future<Result<Unit, IoError>>
```

# ServerWebSocket

**Import:** `import system.webapi;`

Read the [usage guide](../../web-server/websockets.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `ServerWebSocket`

A WebSocket connection offered to a `@websocket` route. The handshake stays open until the handler calls `accept`; closing the socket first, or returning without accepting it, answers the handshake with 403 Forbidden.

```dream
public class ServerWebSocket
```

## `accept`

Completes the WebSocket handshake with 101 Switching Protocols. Call it once, after any checks on the handshake request such as its `Origin` header; messages can be sent and received only after it succeeds. Fails when the request is not a valid WebSocket upgrade, the client disconnected, or the socket was already accepted or closed.

```dream
public fun accept(token: Option<CancellationToken> = Option.None): Future<Result<Unit, IoError>>
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

Sends a normal closing handshake; before `accept`, rejects the connection with 403 Forbidden like `close_with`.

```dream
public fun close(token: Option<CancellationToken> = Option.None): Future<Result<Unit, IoError>>
```

## `close_with`

Sends a closing handshake with a code and reason. Before `accept`, this rejects the connection instead: the handshake is answered with 403 Forbidden when the handler returns, and `code` and `reason` are not sent. Use code 1008 (policy violation) for that case.

```dream
public fun close_with(
    code: int,
    reason: string,
    token: Option<CancellationToken> = Option.None
): Future<Result<Unit, IoError>>
```

# ServerWebSocket

**Import:** `import system.webapi;`

Read the [usage guide](../stdlib/index.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class ServerWebSocket`

Server-side WebSocket after a successful `@websocket` handshake.

```dream
public class ServerWebSocket
```

## `upgrade`

```dream
public static fun upgrade(borrow req: HttpIncoming): Option<ServerWebSocket>
```

## `send_text`

```dream
public async fun send_text(text: string, token: Option<CancellationToken> = Option.None): Result<bool, NetError>
```

## `send_binary`

```dream
public async fun send_binary(data: byte[], token: Option<CancellationToken> = Option.None): Result<bool, NetError>
```

## `receive`

```dream
public async fun receive(token: Option<CancellationToken> = Option.None): Result<WebSocketMessage, NetError>
```

## `close`

```dream
public fun close(): void
```

## `close_with`

```dream
public fun close_with(code: int, reason: string): void
```

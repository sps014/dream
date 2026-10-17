# WebSocketMessage

**Import:** `import system.net;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `WebSocketMessage`

A complete WebSocket data message or the peer's closing handshake.

```dream
public enum WebSocketMessage
```

## `text`

A text message with its payload.

```dream
Text(text: string)
```

## `bytes`

A binary message with its payload.

```dream
Binary(bytes: byte[])
```

## `code`

A close message with its status code and reason.

```dream
Close(code: int, reason: string)
```

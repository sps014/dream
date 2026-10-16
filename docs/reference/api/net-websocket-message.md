# Websocket Message

**Import:** `import system.net;`

## `WebSocketMessage`

A complete WebSocket data message or the peer's closing handshake.

```dream
public enum WebSocketMessage
```

## `Text`

A text message with its payload.

```dream
Text(text: string)
```

## `Binary`

A binary message with its payload.

```dream
Binary(bytes: byte[])
```

## `Close`

A close message with its status code and reason.

```dream
Close(code: int, reason: string)
```

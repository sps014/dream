# Websocket Message

**Import:** `import system.net;`

## `public enum WebSocketMessage`

A complete WebSocket data message or the peer's closing handshake.

```dream
public enum WebSocketMessage
```

## `Text`

```dream
Text(text: string)
```

## `Binary`

```dream
Binary(bytes: byte[])
```

## `Close`

```dream
Close(code: int, reason: string)
```

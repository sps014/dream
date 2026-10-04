# WebSocketMessage

**Import:** `import system.net;`

Read the [usage guide](../stdlib/http.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `enum WebSocketMessage`

A message received from a `WebSocket`.

```dream
public enum WebSocketMessage
```

## `Text(string)`

```dream
Text(string)
```

## `Binary(byte[])`

```dream
Binary(byte[])
```

## `Close(code: int, reason: string)`

The peer closed the connection; `code`/`reason` come from its close frame (`code` defaults to `1000` when the peer didn't send one).

```dream
Close(code: int, reason: string)
```

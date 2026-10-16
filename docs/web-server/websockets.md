---
title: WebSockets
summary: Two-way messaging inside a mounted router.
icon: material/swap-horizontal-bold
---

# WebSockets

Use `@websocket` inside an explicitly mounted router.

```dream
import system;
import system.webapi;

@router("")
class Routes {
    @websocket("/ws")
    public static async fun echo(ws: ServerWebSocket): void {
        await for (let message in ws.messages()) {
            switch (message) {
                Text(text) => {
                    ws.send_text(text).await.unwrap();
                },
                Binary(bytes) => {
                    ws.send_binary(bytes).await.unwrap();
                },
                Close(_, _) => {},
            }
        }
    }
}

async fun main(): void {
    let app = WebApp();
    app.include_router(Routes.router()).unwrap();
    app.run("127.0.0.1", 8000).await.unwrap();
}
```

Connect to `ws://127.0.0.1:8000/ws` with a WebSocket client. A handler must accept `ServerWebSocket`; the generator performs the upgrade before invoking it. Sockets are native server resources and close on cancellation or app shutdown.

## Socket methods

Every method takes an optional trailing `CancellationToken`.

| `ServerWebSocket` method | Returns | Effect |
| --- | --- | --- |
| `messages()` | `AsyncIterable<WebSocketMessage>` | Yields each message through the final `Close`. A transport failure ends iteration. |
| `receive()` | `Future<Result<WebSocketMessage, IoError>>` | Waits for one complete message. |
| `send_text(text)` | `Future<Result<bool, IoError>>` | Sends a text message. |
| `send_binary(bytes)` | `Future<Result<bool, IoError>>` | Sends a binary message. |
| `close()` | `Future<Result<bool, IoError>>` | Sends a normal closing handshake. |
| `close_with(code, reason)` | `Future<Result<bool, IoError>>` | Sends a closing handshake with a status code and reason. |

| `WebSocketMessage` variant | Payload |
| --- | --- |
| `Text(text)` | `string` |
| `Binary(bytes)` | `byte[]` |
| `Close(code, reason)` | `int` close code and `string` reason |

A route may use dependencies and middleware before upgrade. After upgrading, HTTP response status changes are no longer available. WebSocket operations are absent from ordinary OpenAPI paths. The client-side `WebSocket` API belongs to `system.net` and supports its documented native, Node, and browser targets.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).

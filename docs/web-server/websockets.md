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
        ws.accept().await.unwrap();

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

Connect to `ws://127.0.0.1:8000/ws` with a WebSocket client. The handler must take a `ServerWebSocket` parameter.

Call `accept()` before you send or receive. That completes the handshake with 101 Switching Protocols, and the handler runs before the handshake finishes.

A request that is not a WebSocket upgrade gets 400 Bad Request, and the handler does not run. The socket closes when the handler is cancelled or the app shuts down.

## Checking Origin

Browsers send the page's cookies on a WebSocket handshake from any site, and CORS does not apply. If the socket uses cookie authentication, check `Origin` before `accept()`, because the server does not check it for you:

```dream
import system;
import system.webapi;

@router("")
class ChatRoutes {
    @websocket("/chat")
    public static async fun chat(ws: ServerWebSocket, req: HttpIncoming): void {
        let origin = req.headers.get("Origin").unwrap_or("");
        if origin != "https://chat.example.com" {
            ws.close_with(1008, "origin not allowed").await.unwrap();
            return;
        }

        ws.accept().await.unwrap();

        await for (let message in ws.messages()) {
            switch (message) {
                Text(text) => {
                    ws.send_text("you said: " + text).await.unwrap();
                },
                _ => {},
            }
        }
    }
}
```

Closing the socket before `accept()`, or returning without calling it, rejects the handshake with 403 Forbidden, and the browser reports a failed connection. Command-line clients may omit `Origin`, so decide whether a missing header is allowed.

## Socket methods

Every method takes an optional trailing `CancellationToken`.

| `ServerWebSocket` method | Returns | Effect |
| --- | --- | --- |
| `accept()` | `Future<Result<Unit, IoError>>` | Completes the handshake. Fails when the client disconnected or the socket was already accepted or closed. |
| `messages()` | `AsyncIterable<WebSocketMessage>` | Yields each message through the final `Close`. A transport failure ends iteration. |
| `receive()` | `Future<Result<WebSocketMessage, IoError>>` | Waits for one complete message. |
| `send_text(text)` | `Future<Result<Unit, IoError>>` | Sends a text message. |
| `send_binary(bytes)` | `Future<Result<Unit, IoError>>` | Sends a binary message. |
| `close()` | `Future<Result<Unit, IoError>>` | Sends a normal closing handshake. Before `accept()`, rejects the handshake with 403. |
| `close_with(code, reason)` | `Future<Result<Unit, IoError>>` | Sends a closing handshake with a status code and reason. Before `accept()`, rejects the handshake with 403. |

Sending or receiving before `accept()` fails with error code `EINVAL`.

| `WebSocketMessage` variant | Payload |
| --- | --- |
| `Text(text)` | `string` |
| `Binary(bytes)` | `byte[]` |
| `Close(code, reason)` | `int` close code and `string` reason |

## Limits and keepalive

`ServerConfig` sets the limits for every server WebSocket; see [Server settings](configuration.md#server-settings-serverconfig).

| Event | What the handler sees |
| --- | --- |
| A message larger than `ws_max_size` (16 MiB by default) | `Close(1009, "message too big")`, then the socket ends. |
| No answer to a server ping within `ws_ping_timeout_ms` | `Close(1011, "keepalive ping timeout")`, then the socket ends. |
| The app shuts down | The server sends the client `1012` (service restart); the client's reply arrives as `Close`. |
| The handler falls `ws_max_queue` messages behind | The server stops reading from the client until the handler catches up. |

The server pings every `ws_ping_interval_ms` (20 seconds by default) and answers client pings itself, so idle connections stay open through proxies and dead clients are detected.

A route may use dependencies and middleware before the upgrade. After the upgrade, you cannot change the HTTP status.

WebSocket operations do not appear as ordinary OpenAPI paths. The client `WebSocket` type is in `system.http` and works on its documented native, Node, and browser targets.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).

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

Connect to `ws://127.0.0.1:8000/ws` with a WebSocket client. A handler must take a `ServerWebSocket` parameter. The connection is offered to the handler before the handshake completes: call `accept()` to answer with 101 Switching Protocols, and only then send or receive messages. A request to the route that is not a WebSocket upgrade gets 400 Bad Request without running the handler. Sockets are native server resources and close on cancellation or app shutdown.

## Checking Origin

Browsers attach the page's cookies to a WebSocket handshake no matter which site opened it, and CORS does not apply to WebSockets. A socket that relies on cookie authentication is therefore open to cross-site WebSocket hijacking unless the handler checks the `Origin` header. The server performs no automatic Origin check, so do it before `accept()`:

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

Closing the socket before `accept()` rejects the handshake with 403 Forbidden, and the browser reports a failed connection. A handler that returns without calling `accept()` rejects it the same way. Clients that are not browsers, such as command-line tools, may omit `Origin` entirely, so decide whether a missing header is allowed for your endpoint.

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

A route may use dependencies and middleware before upgrade. After upgrading, HTTP response status changes are no longer available. WebSocket operations are absent from ordinary OpenAPI paths. The client-side `WebSocket` API belongs to `system.net` and supports its documented native, Node, and browser targets.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).

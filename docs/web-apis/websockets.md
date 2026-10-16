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

`messages()` yields text, binary, and a final Close message. Transport failure ends iteration. You can also call `receive`, `send_text`, `send_binary`, `close`, and `close_with`, each with optional cancellation where applicable and explicit `IoError` results.

A route may use dependencies and middleware before upgrade. After upgrading, HTTP response status changes are no longer available. WebSocket operations are absent from ordinary OpenAPI paths. The client-side `WebSocket` API belongs to `system.net` and supports its documented native, Node, and browser targets.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).

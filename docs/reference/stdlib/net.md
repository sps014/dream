# Raw sockets

**Import:** `import system.net;` (same package as [HTTP](http.md))

Use `TcpClient` or `WebSocket` when you need a lasting connection or a non-HTTP protocol. Use `HttpClient` for ordinary request/response.

```dream
import system;
import system.net;

async fun main(): void {
    switch (await TcpClient.connect("127.0.0.1", 9000)) {
        Ok(client) => {
            await client.send_text("ping");
            client.close();
        },
        Err(e) => System.println(e.code()),
    }
}
```

| Runtime | TCP | WebSocket |
| --- | --- | --- |
| Native C (`dream run`) | real socket | `ws://` and `wss://` |
| Node | real socket | `ws://` and `wss://` |
| Browser | compile error | page `WebSocket` |

## `TcpClient`

`connect` / `connect_timeout`, then `send` / `send_text`, `receive(max_bytes)`, `close()`. Optional last `token` on the async methods; cancelled → `NetError` `ECANCELLED`.

## `WebSocket`

`connect` / `connect_timeout`, `send_text` / `send_binary`, `receive()` → `WebSocketMessage`, `close` / `close_with(code, reason)`. Optional last `token` on the async methods; cancelled → `NetError` `ECANCELLED`.

Failures are `NetError`.

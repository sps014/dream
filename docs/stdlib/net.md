# Raw sockets

**Package:** `system.net` — `import system.net;` (same package as [HTTP](http.md))

`TcpClient` and `WebSocket` give you raw, unbuffered sockets underneath the request/response
`HttpClient`. Reach for these when you need a persistent connection, a non-HTTP wire protocol, or
full control over framing; reach for `HttpClient` for ordinary request/response calls.

```dream
import system;
import system.net;
```

## Platform notes

| Runtime | `TcpClient` | `WebSocket` |
| --- | --- | --- |
| Native (`dream run`) | Real TCP socket | `ws://` only (plain TCP + handshake); `wss://` resolves `Err(NetError.unsupported(...))` |
| Node.js | Real TCP socket (`node:net`) | Standard `WebSocket` (Node ≥ 22, or a polyfill assigned onto `globalThis.WebSocket`); both schemes |
| Browser (`--web`) | Compile error — `TcpClient` is `@native` / `@node` only | Standard `WebSocket`; both schemes |

## `TcpClient`

#### `await TcpClient.connect(host, port): Result<TcpClient, NetError>`

Opens a TCP connection with no connect timeout.

```dream
switch (await TcpClient.connect("127.0.0.1", 9000)) {
    Ok(client) => { /* ... */ },
    Err(e) => System.println(e.code()),
}
```

#### `await TcpClient.connect_timeout(host, port, timeout_ms): Result<TcpClient, NetError>`

Same as `connect`, but fails with `NetError.connect_failed` if the handshake doesn't finish within `timeout_ms` milliseconds.

```dream
let result = await TcpClient.connect_timeout("127.0.0.1", 9000, 3000);
```

#### `await send(data: byte[]): Result<int, NetError>` / `await send_text(text: string): Result<int, NetError>`

Writes raw bytes or UTF-8 text; resolves with the number of bytes written.

```dream
switch (client.send_text("PING\n")) {
    Ok(n) => System.println(n),
    Err(e) => System.println(e.message()),
}
```

#### `await receive(max_bytes: int): Result<byte[], NetError>`

Reads up to `max_bytes`. An **empty array means the peer closed the connection (EOF)**, not an error — check `data.length == 0` to detect end-of-stream, the same convention as `HttpStreamResponse.read_chunk`.

```dream
switch (await client.receive(4096)) {
    Ok(data) => {
        if (data.length == 0) {
            System.println("peer closed the connection");
        }
    },
    Err(e) => System.println(e.message()),
}
```

#### `close(): void`

Closes the connection. Safe to call more than once.

## `WebSocket`

#### `await WebSocket.connect(url): Result<WebSocket, NetError>` / `connect_timeout(url, timeout_ms)`

Opens a WebSocket connection, optionally with a connect timeout in milliseconds (`0` = none).

```dream
switch (await WebSocket.connect("ws://localhost:8080/chat")) {
    Ok(socket) => { /* ... */ },
    Err(e) => System.println(e.code()),
}
```

#### `await send_text(text: string): Result<bool, NetError>` / `await send_binary(data: byte[]): Result<bool, NetError>`

Sends a text or binary frame; `Ok(true)` on success.

```dream
await socket.send_text("hello");
await socket.send_binary(Buffer.alloc<byte>(4));
```

#### `await receive(): Result<WebSocketMessage, NetError>`

Waits for the next message. Resolves `NetError` on a transport failure; a **peer-initiated close
arrives as `Ok(WebSocketMessage.Close(...))`**, not an error — check the variant, not just `Result`.

```dream
switch (await socket.receive()) {
    Ok(msg) => {
        switch (msg) {
            Text(text) => System.println(text),
            Binary(data) => System.println(data.length),
            Close(code, reason) => System.println("closed: " + code.to_string()),
        }
    },
    Err(e) => System.println(e.message()),
}
```

#### `close(): void` / `close_with(code: int, reason: string): void`

Closes the connection, optionally with an explicit close code (default `1000`) and reason. Safe to call more than once.

## `WebSocketMessage`

```dream
public enum WebSocketMessage {
    Text(text: string),
    Binary(data: byte[]),
    Close(code: int, reason: string),
}
```

`Close.code` defaults to `1000` when the peer didn't send an explicit code.

## `NetError`

Implements [`Error`](option-result.md).

```dream
let e = NetError.connect_failed("connection refused");
System.println(e.code());     // ECONNECT
System.println(e.message());  // connection refused
```

| `code()` | Meaning |
| --- | --- |
| `ECONNECT` | The connection could not be established (DNS, refused, timeout, TLS, ...) |
| `EIO` | A read/write against an open connection failed |
| `ECLOSED` | The operation was attempted after the connection was already closed |
| `EPROTOCOL` | The peer sent malformed/unexpected protocol data |
| `EUNSUPPORTED` | Operation not supported on the current host (e.g. `wss://` on native — see platform notes) |

See [HTTP](http.md) for request/response networking (`HttpClient`, `HttpStreamResponse`).

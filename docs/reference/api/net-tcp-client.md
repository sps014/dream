# TcpClient

**Import:** `import system.net;`

Read the [usage guide](../stdlib/http.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class TcpClient`

A raw TCP client. Native and Node hosts back this with a real socket; the browser has no raw socket API, so `connect`/`connect_timeout` resolve `Err(NetError.unsupported(...))` there.

```dream
public class TcpClient
```

## `connect`

Opens a TCP connection to `host:port` with no connect timeout.

```dream
public static async fun connect(host: string, port: int, token: Option<CancellationToken> = Option.None): Result<TcpClient, NetError>
```

## `connect_timeout`

Opens a TCP connection with a connect timeout in milliseconds (`0` = none).

```dream
public static async fun connect_timeout(host: string, port: int, timeout_ms: int, token: Option<CancellationToken> = Option.None): Result<TcpClient, NetError>
```

## `send`

Sends raw bytes; resolves with the number of bytes written.

```dream
public async fun send(data: byte[], token: Option<CancellationToken> = Option.None): Result<int, NetError>
```

## `send_text`

Sends `text` encoded as UTF-8; resolves with the number of bytes written.

```dream
public async fun send_text(text: string, token: Option<CancellationToken> = Option.None): Result<int, NetError>
```

## `receive`

Reads up to `max_bytes`. An empty array means the peer closed the connection (EOF), not an error — check `data.length == 0` to detect end-of-stream.

```dream
public async fun receive(max_bytes: int, token: Option<CancellationToken> = Option.None): Result<byte[], NetError>
```

## `close`

Closes the connection. Safe to call more than once.

```dream
public fun close(): void
```

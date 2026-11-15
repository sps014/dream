---
title: Raw sockets and compression
summary: "TCP and UDP sockets, plus gzip and deflate streams."
icon: material/lan
api: reference/api/system-net.md
---

# Raw sockets and compression

## TCP and UDP

`TcpListener`, `TcpStream`, and `UdpSocket` work on native and Node. Browsers report `SocketCapabilities.supported == false` and return `EUNSUPPORTED`; use the HTTP client there.

```dream
import system;
import system.net;

async fun main(): void {
    let listener = TcpListener.bind("127.0.0.1", 0).await.unwrap();
    let connecting = TcpStream.connect("127.0.0.1", listener.local_port);
    Promise.start(connecting);
    let server = listener.accept().await.unwrap();
    let client = connecting.await.unwrap();
    client.write_all_async(Encoding.utf8_encode("hello")).await.unwrap();
    client.shutdown_write().unwrap();
    let text = "";
    while true {
        let bytes = server.read_async(1024).await.unwrap();
        if bytes.length == 0 {
            break;
        }
        text = text + Encoding.utf8_decode(bytes);
    }
    System.println(text);
    server.close().unwrap();
    client.close().unwrap();
    listener.close().unwrap();
}
```

TCP streams implement `AsyncReader` and `AsyncWriter`. Reads and writes may return only some of the bytes.

- Use `write_all_async` or `Streams.copy_async` to write every byte.
- A zero-length read consumes nothing. An empty read when you asked for bytes means the peer closed.
- `shutdown_write()` sends FIN after accepted writes and keeps reading open. Native and Node allow a half-open connection.
- A second read, or a second write, at the same time returns `EBUSY`. One read and one write may run together.
- A read larger than 1 MiB returns at most 1 MiB. A write has no size cap and returns how many bytes were accepted.
- `set_nodelay` turns `TCP_NODELAY` on or off.

`connect`, `accept`, and `read_with_timeout` take a millisecond deadline. Zero means no deadline.

- Async methods use the current task's cancellation unless you pass a token. Bytes already transferred stay transferred.
- `close()` wakes pending operations and is safe to call again. An accepted stream stays valid after its listener closes.
- Node queues at most 64 accepted sockets and rejects more. The native listener uses the operating system's backlog.

Async methods below also accept an optional trailing `token: Option<CancellationToken>`.

| `TcpListener` member | Returns | Effect |
| --- | --- | --- |
| `TcpListener.bind(host, port)` | `Result<TcpListener, IoError>` (async) | Listens on `host`; port `0` picks a free port. |
| `local_host` / `local_port` | `string` / `int` | The bound address and actual port. |
| `accept(timeout_ms = 0)` | `Result<TcpStream, IoError>` (async) | Waits for the next connection. |
| `close()` | `Result<Unit, IoError>` | Stops listening and cancels pending accepts. |

| `TcpStream` member | Returns | Effect |
| --- | --- | --- |
| `TcpStream.connect(host, port, timeout_ms = 0)` | `Result<TcpStream, IoError>` (async) | Opens a connection. |
| `local_host` / `local_port` | `string` / `int` | This end's address and port. |
| `peer_host` / `peer_port` | `string` / `int` | The remote end's address and port. |
| `read_async(max_bytes)` | `Result<byte[], IoError>` (async) | Reads up to `max_bytes`; empty means the peer closed. |
| `read_with_timeout(max_bytes, timeout_ms)` | `Future<Result<byte[], IoError>>` | Same as `read_async` with a deadline. |
| `write_async(bytes)` | `Result<int, IoError>` (async) | Writes some bytes and returns the count. |
| `write_all_async(bytes)` | `Result<Unit, IoError>` (async) | Retries until every byte is written. |
| `flush_async()` | `Result<Unit, IoError>` (async) | Checks the socket is open; writes are not buffered. |
| `shutdown_write()` | `Result<Unit, IoError>` | Ends the sending direction and keeps reading open. |
| `set_nodelay(enabled)` | `Result<Unit, IoError>` | Turns `TCP_NODELAY` on or off. |
| `close()` | `Result<Unit, IoError>` | Closes the socket and wakes pending operations. |

`send(host, port, data)` sends one datagram, including an empty one. `receive` returns a `UdpDatagram` with `data`, the sender `host` and `port`, and `truncated`.

- An empty message is valid. It does not mean the socket closed.
- Sends are at most 65507 bytes, and a receive accepts up to 65535. One receive can be pending per socket.
- `set_broadcast` and `set_ttl` set those options. Node keeps at most 64 queued datagrams and drops the rest.
- A deadline, cancellation, or `close()` wakes a pending receive.

| `UdpSocket` member | Returns | Effect |
| --- | --- | --- |
| `UdpSocket.bind(host, port)` | `Result<UdpSocket, IoError>` (async) | Binds a socket; port `0` picks a free port. |
| `local_host` / `local_port` | `string` / `int` | The bound address and actual port. |
| `send(host, port, data, timeout_ms = 0)` | `Result<int, IoError>` (async) | Sends one datagram of at most 65507 bytes. |
| `receive(max_bytes = 65535, timeout_ms = 0)` | `Result<UdpDatagram, IoError>` (async) | Waits for one datagram. |
| `set_broadcast(enabled)` | `Result<Unit, IoError>` | Allows or forbids sending to broadcast addresses. |
| `set_ttl(ttl)` | `Result<Unit, IoError>` | Sets the outgoing time-to-live, from 1 to 255. |
| `close()` | `Result<Unit, IoError>` | Closes the socket and wakes pending receives. |

| `UdpDatagram` field | Type | Meaning |
| --- | --- | --- |
| `data` | `byte[]` | Received bytes, at most the requested size. |
| `host` | `string` | Sender's numeric address. |
| `port` | `int` | Sender's port. |
| `truncated` | `bool` | True when the message was larger than the receive buffer. |

## Streaming gzip and zlib

`system.compress` reads and writes gzip and zlib (DEFLATE) streams that match other tools, on every target, with nothing extra to deploy.

A wrapper borrows the underlying stream. It works in chunks and stops at `max_output_bytes` (default 64 MiB, at most 1 GiB).

```dream
import system;
import system.io;
import system.compress;

fun main(): void {
    let encoded = MemoryStream();
    let writer = Compression.writer(encoded, CompressionFormat.Gzip).unwrap();
    writer.write_all(Encoding.utf8_encode("hello compression")).unwrap();
    writer.finish().unwrap();
    encoded.seek(0L).unwrap();
    let reader = Compression.reader(encoded, CompressionFormat.Gzip).unwrap();
    let decoded = MemoryStream();
    Streams.copy(reader, decoded).unwrap();
    decoded.seek(0L).unwrap();
    System.println(Encoding.utf8_decode(decoded.read(100).unwrap()));
    reader.close();
}
```

| Member | Returns | Effect |
| --- | --- | --- |
| `Compression.writer(destination, format = CompressionFormat.Gzip, max_output_bytes = 67108864L)` | `Result<CompressionWriter, IoError>` | Creates an encoder writing to `destination`. |
| `Compression.reader(source, format = CompressionFormat.Gzip, max_output_bytes = 67108864L)` | `Result<CompressionReader, IoError>` | Creates a decoder reading from `source`. |
| `writer.write(bytes)` | `Result<int, IoError>` | Accepts up to 64 KiB and returns how many bytes it consumed. Compressed blocks reach the destination as they fill. |
| `writer.flush()` | `Result<Unit, IoError>` | Writes pending data without ending the stream. |
| `writer.finish()` | `Result<Unit, IoError>` | Writes the trailer and flushes. |
| `reader.read(max_bytes)` | `Result<byte[], IoError>` | Returns decoded bytes; empty at the end of the stream. |
| `close()` | `void` | Stops the wrapper without closing the borrowed stream; later calls fail. |

| `CompressionFormat` | Framing |
| --- | --- |
| `Gzip` | Gzip framing, verified with a CRC and the uncompressed size. |
| `Zlib` | Zlib framing, verified with an Adler-32 checksum. |

A `max_output_bytes` outside 0 to 1 GiB returns `EINVAL`; corrupt, truncated,
oversized, or trailing input returns `ECOMPRESS`.

`CompressionWriter` implements `Writer`. `flush()` writes pending bytes and flushes the destination, and `finish()` writes the trailer and flushes.

`close()` stops the wrapper and does not close or finish the destination. Dropping a writer does the same, so call `finish()` when you need trailer and flush errors.

`CompressionReader` implements `Reader` and checks the checksum and that the stream ended cleanly.

Corruption, truncation, output past the limit, a second member, and trailing bytes are errors. EOF succeeds only when the source ends right after one stream; bytes already decoded stay consumed if a later check fails.

## Collection capacity

`List`, `Queue`, `Map`, and `Set` have `reserve(additional)`, which returns `Result<bool, CapacityError>`. Values and length stay the same.

- A hash table request includes load-factor room and can reuse deleted slots.
- A negative or unsupported count returns an error and does not change capacity.
- If allocation fails, it fails the same way as any other allocation.
- `capacity` is physical slots or buckets, not a count of insertions that are guaranteed to fit.

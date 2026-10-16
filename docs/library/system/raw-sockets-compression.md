---
title: Raw sockets and compression
summary: "TCP and UDP sockets, plus gzip and deflate streams."
icon: material/lan
api: reference/api/system-net.md
---

# Raw sockets and compression

## TCP and UDP

`system.net` provides owned `TcpListener`, `TcpStream`, and `UdpSocket` resources on native and Node. `SocketCapabilities.supported` is false in browsers, where raw socket requests return `EUNSUPPORTED`. HTTP remains available through the separate HTTP capabilities.

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

TCP streams implement `AsyncReader` and `AsyncWriter`. Reads and writes may be partial. Use `write_all_async` or `Streams.copy_async` to retry writes. Zero-length reads consume nothing; positive-size empty reads denote peer EOF. `shutdown_write()` sends FIN after accepted writes and retains the read side. Native and Node support half-open connections. A concurrent read or write of the same direction returns `EBUSY`; a read and write may proceed together. TCP operations accept up to 16 MiB per call. `set_nodelay` controls TCP_NODELAY.

`connect`, `accept`, and `read_with_timeout` expose millisecond deadlines; zero disables a deadline. Async methods inherit structured cancellation unless an explicit token is passed. Cancellation preserves completed side effects. Close wakes pending operations and is idempotent. Accepted streams remain valid after their listener closes. Node bounds queued accepted sockets to 64 and rejects excess pending connections; the native listener uses the OS backlog.

Async methods below also accept an optional trailing `token: Option<CancellationToken>`.

| `TcpListener` member | Returns | Effect |
| --- | --- | --- |
| `TcpListener.bind(host, port)` | `Result<TcpListener, IoError>` (async) | Listens on `host`; port `0` picks a free port. |
| `local_host` / `local_port` | `string` / `int` | The bound address and actual port. |
| `accept(timeout_ms = 0)` | `Result<TcpStream, IoError>` (async) | Waits for the next connection. |
| `close()` | `Result<bool, IoError>` | Stops listening and cancels pending accepts. |

| `TcpStream` member | Returns | Effect |
| --- | --- | --- |
| `TcpStream.connect(host, port, timeout_ms = 0)` | `Result<TcpStream, IoError>` (async) | Opens a connection. |
| `local_host` / `local_port` | `string` / `int` | This end's address and port. |
| `peer_host` / `peer_port` | `string` / `int` | The remote end's address and port. |
| `read_async(max_bytes)` | `Result<byte[], IoError>` (async) | Reads up to `max_bytes`; empty means the peer closed. |
| `read_with_timeout(max_bytes, timeout_ms)` | `Future<Result<byte[], IoError>>` | Same as `read_async` with a deadline. |
| `write_async(bytes)` | `Result<int, IoError>` (async) | Writes some bytes and returns the count. |
| `write_all_async(bytes)` | `Result<bool, IoError>` (async) | Retries until every byte is written. |
| `flush_async()` | `Result<bool, IoError>` (async) | Checks the socket is open; writes are not buffered. |
| `shutdown_write()` | `Result<bool, IoError>` | Ends the sending direction and keeps reading open. |
| `set_nodelay(enabled)` | `Result<bool, IoError>` | Turns `TCP_NODELAY` on or off. |
| `close()` | `Result<bool, IoError>` | Closes the socket and wakes pending operations. |

UDP `send(host, port, data)` sends one message, including an empty message. `receive(max_bytes)` returns `UdpDatagram` with `data`, sender `host` and `port`, and an explicit `truncated` flag. Empty messages are valid and never denote EOF. The portable send limit is 65507 bytes; receive buffers support up to 65535 bytes. One receive can be pending per socket. `set_broadcast` and `set_ttl` expose supported options. Node retains up to 64 queued datagrams and drops excess arrivals, consistent with UDP's lossy transport contract. Deadlines, cancellation, and closure wake pending receives.

| `UdpSocket` member | Returns | Effect |
| --- | --- | --- |
| `UdpSocket.bind(host, port)` | `Result<UdpSocket, IoError>` (async) | Binds a socket; port `0` picks a free port. |
| `local_host` / `local_port` | `string` / `int` | The bound address and actual port. |
| `send(host, port, data, timeout_ms = 0)` | `Result<int, IoError>` (async) | Sends one datagram of at most 65507 bytes. |
| `receive(max_bytes = 65535, timeout_ms = 0)` | `Result<UdpDatagram, IoError>` (async) | Waits for one datagram. |
| `set_broadcast(enabled)` | `Result<bool, IoError>` | Allows or forbids sending to broadcast addresses. |
| `set_ttl(ttl)` | `Result<bool, IoError>` | Sets the outgoing time-to-live, from 1 to 255. |
| `close()` | `Result<bool, IoError>` | Closes the socket and wakes pending receives. |

| `UdpDatagram` field | Type | Meaning |
| --- | --- | --- |
| `data` | `byte[]` | Received bytes, at most the requested size. |
| `host` | `string` | Sender's numeric address. |
| `port` | `int` | Sender's port. |
| `truncated` | `bool` | True when the message was larger than the receive buffer. |

## Streaming gzip and zlib

`system.compress` uses the same Rust `flate2`/`zlib-rs` adapter on native, Node, and browser hosts. Wrappers borrow their underlying streams. Encoding and decoding operate in bounded chunks and enforce a total output limit, defaulting to 64 MiB and configurable up to 1 GiB.

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
| `writer.write(bytes)` | `Result<int, IoError>` | Compresses up to 64 KiB and returns how many bytes it consumed. |
| `writer.flush()` | `Result<bool, IoError>` | Writes pending data without ending the stream. |
| `writer.finish()` | `Result<bool, IoError>` | Writes the trailer and flushes. |
| `reader.read(max_bytes)` | `Result<byte[], IoError>` | Returns decoded bytes; empty at the end of the stream. |
| `close()` | `void` | Discards codec state without closing the borrowed stream. |

| `CompressionFormat` | Framing |
| --- | --- |
| `Gzip` | Gzip framing, verified with a CRC and the uncompressed size. |
| `Zlib` | Zlib framing, verified with an Adler-32 checksum. |

A `max_output_bytes` outside 0 to 1 GiB returns `EINVAL`; corrupt, truncated,
oversized, or trailing input returns `ECOMPRESS`.

`CompressionWriter` implements `Writer`. `flush()` emits a synchronization boundary and flushes its destination. `finish()` emits the trailer, releases encoder state, and flushes. `close()` abandons codec state without closing or finishing the borrowed destination. Dropping a writer also abandons it, so call `finish()` to observe trailer and flush failures.

`CompressionReader` implements `Reader` and verifies checksums and complete stream termination. Corruption, truncation, expansion beyond the configured limit, concatenated members, and trailing bytes return errors. Successful EOF requires the underlying source to reach EOF after exactly one compressed stream. A failure can occur after previously decoded bytes have been consumed; those side effects are preserved.

## Collection capacity

`List`, `Queue`, `Map`, and `Set` provide `reserve(additional) -> Result<bool, CapacityError>`. Reservation preserves values and length. For hash tables it accounts for the load factor and can reclaim tombstones. Negative and unsupported requests return errors before changing capacity. Allocation failure follows the runtime's ordinary allocation policy. `capacity` describes physical element slots or hash buckets, rather than a guaranteed insertion count.

UUID generation adds no speculative cryptographic algorithms. Further cryptography remains driven by concrete application requirements.

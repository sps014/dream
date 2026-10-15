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
        if bytes.length == 0 { break; }
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

UDP `send(host, port, data)` sends one message, including an empty message. `receive(max_bytes)` returns `UdpDatagram` with `data`, sender `host` and `port`, and an explicit `truncated` flag. Empty messages are valid and never denote EOF. The portable send limit is 65507 bytes; receive buffers support up to 65535 bytes. One receive can be pending per socket. `set_broadcast` and `set_ttl` expose supported options. Node retains up to 64 queued datagrams and drops excess arrivals, consistent with UDP's lossy transport contract. Deadlines, cancellation, and closure wake pending receives.

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

`CompressionWriter` implements `Writer`. `flush()` emits a synchronization boundary and flushes its destination. `finish()` emits the trailer, releases encoder state, and flushes. `close()` abandons codec state without closing or finishing the borrowed destination. Dropping a writer also abandons it, so call `finish()` to observe trailer and flush failures.

`CompressionReader` implements `Reader` and verifies checksums and complete stream termination. Corruption, truncation, expansion beyond the configured limit, concatenated members, and trailing bytes return errors. Successful EOF requires the underlying source to reach EOF after exactly one compressed stream. A failure can occur after previously decoded bytes have been consumed; those side effects are preserved.

## Collection capacity

`List`, `Queue`, `Map`, and `Set` provide `reserve(additional) -> Result<bool, CapacityError>`. Reservation preserves values and length. For hash tables it accounts for the load factor and can reclaim tombstones. Negative and unsupported requests return errors before changing capacity. Allocation failure follows the runtime's ordinary allocation policy. `capacity` describes physical element slots or hash buckets, rather than a guaranteed insertion count.

UUID generation adds no speculative cryptographic algorithms. Further cryptography remains driven by concrete application requirements.

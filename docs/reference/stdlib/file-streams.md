# Read and write streams

Import `system.io` for file handles, memory streams, buffering, and the `Reader`,
`Writer`, `AsyncReader`, and `AsyncWriter` interfaces.

## File handles and cleanup

File handles support `r`, `w`, `a`, `r+`, `w+`, and `a+`. Reads, writes, seeking,
length queries, and explicit closure return `Result`. A successful empty read at a
positive requested size means EOF; a read failure returns `IoError`.

```dream
import system.io;

fun main(): Result<bool, IoError> {
    let file = FileHandle.open("input.txt", "r")?;
    while true {
        let line = file.read_line()?;
        if line.is_none() { break; }
    }
    return file.close();
}
```

`close()` is idempotent. Operations after closure return `ECLOSED`. Dropping the
last reference also closes the handle, but only explicit closure can report an
error. A failed close retires the handle: retrying an OS descriptor could close a
different file after the descriptor has been reused.

File positions and lengths use `long`. `tell()` includes buffered read-ahead, so
mixing line and byte reads preserves the logical position. Failed seeks preserve
the position and any unread bytes. `FileStream.position` and `.length` return
`Result<long, IoError>`; use `seek()` to change position.

Async file methods currently execute synchronous host operations and check tokens
before execution. They do not yet provide nonblocking file I/O or interrupt an
operation already running.

## Streaming copies

```dream
import system.io;

fun main(): Result<bool, IoError> {
    let source = FileHandle.open("input.bin", "r")?;
    let destination = FileHandle.open("output.bin", "w")?;
    Streams.copy(source, destination, buffer_size: 65536)?;
    destination.close()?;
    return source.close();
}
```

`Streams.copy` returns the byte count and flushes the destination after reaching
EOF. `Reader.read_exact` reports `EEOF` if the stream ends early. `Writer.write_all`
retries short writes and reports an error if the writer makes no progress.
`Streams.copy_async` accepts asynchronous stream implementations and a cancellation
token. Interface implementations must return no more bytes than requested.

## Buffered and memory streams

```dream
import system.io;
import system.encoding;

fun main(): Result<bool, IoError> {
    let source = MemoryStream(Encoding.utf8_encode("first\r\nsecond\n"));
    let reader = BufferedReader(source, buffer_size: 8192);
    let first = reader.read_line()?;
    reader.seek(0L)?;
    let destination = MemoryStream();
    let writer = BufferedWriter(destination, buffer_size: 8192);
    writer.write_all(Encoding.utf8_encode("hello"))?;
    writer.finish()?;
    return destination.close();
}
```

Buffer wrappers retain their underlying streams without closing them. Finish a
buffered writer explicitly so failures are visible; dropping it does not flush.
After a partial flush failure, the unwritten bytes remain available for retry.
`finish()` is idempotent after success and rejects subsequent writes.

Use a buffered reader's `seek()` when repositioning its seekable source. Do not
read, write, or reposition that source directly while unread bytes are held by the
wrapper. `take_buffered()` transfers those unread bytes to the caller.

Line reads support LF and CRLF, preserve standalone carriage returns, and return
an unterminated final line before EOF. The default line limit is 1 MiB, configurable
with `read_line(max_bytes: ...)`. A limit error consumes the prefix and the byte
that exceeded the limit. Buffer sizes must be positive; invalid sizes return
`EINVAL` when used.

Memory streams support seeking, overwriting, and zero-filled gaps. Their
`to_bytes()` method returns an independent copy. Memory stream async methods are
immediately ready and do not block on host I/O.

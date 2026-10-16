---
title: Streams
summary: Read and write data a piece at a time.
icon: material/waves
api: reference/api/system-io.md
---

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
        if line.is_none() {
            break;
        }
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

Async opens, reads, writes, and seeks use a bounded native executor or Node's
callback-based filesystem APIs. Native execution uses four workers and a queue
of at most 128 jobs; a saturated executor reports `EBUSY`. Browser memory-file
operations remain immediately ready. No guest buffers are accessed by workers.

A handle permits one pending operation at a time. Concurrent I/O or explicit
closure returns `EBUSY`, leaving the handle open. After a cancelled operation,
wait for underlying work to finish before reusing or closing that handle. Dropping
the handle is safe while work continues: the host retains the necessary resources.

Tokens are checked before submission and register event subscriptions while
pending. Completion and cancellation wake the scheduler without periodic checks. Cancellation does not undo completed writes or restore consumed input.
A cancelled open closes its late result automatically. A submitted seek completes
and updates read-ahead state before reporting cancellation; this prevents a
cancelled seek from leaving the logical cursor inconsistent.

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

## Asynchronous file copies

```dream
import system.io;
import system.encoding;

async fun main(): Result<bool, IoError> {
    let source = MemoryStream(Encoding.utf8_encode("hello"));
    let destination = FileHandle.open_async("async-copy.txt", "w+").await?;
    Streams.copy_async(source, destination).await?;
    destination.seek_async(0L).await?;
    let bytes = destination.read_async(5).await?;
    destination.close()?;
    source.close()?;
    File.delete("async-copy.txt");
    return Result.Ok(bytes.length == 5);
}
```

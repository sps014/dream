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

Open a file with `FileHandle.open(path, mode)` or `FileHandle.open_async(path, mode, token)`:

| Mode | Access | If the file exists | If it is missing |
|---|---|---|---|
| `FileMode.Read` | read | Opens at the start | `ENOENT` |
| `FileMode.Write` | write | Truncates it | Creates it |
| `FileMode.Append` | append | Writes go to the end | Creates it |
| `FileMode.ReadWrite` | read and write | Opens at the start | `ENOENT` |
| `FileMode.WriteRead` | read and write | Truncates it | Creates it |
| `FileMode.AppendRead` | read and append | Writes go to the end | Creates it |

The mode is a `FileMode` enum. Reads, writes, seeking, length queries, and
explicit closure return `Result`. A successful empty read at a positive requested
size means EOF; a read failure returns `IoError`.

```dream
import system.io;

fun main(): Result<bool, IoError> {
    let file = FileHandle.open("input.txt", FileMode.Read)?;
    while true {
        let line = file.read_line()?;
        if line.is_none() {
            break;
        }
    }
    return file.close();
}
```

| `FileHandle` member | Returns | Effect |
|---|---|---|
| `read(n)` | `Result<byte[], IoError>` | Reads up to `n` bytes. |
| `read_line(max_bytes)` | `Result<Option<string>, IoError>` | Reads one line; `None` at EOF. |
| `write(data)` | `Result<int, IoError>` | Writes bytes; returns the count written. |
| `write_text(text)` | `Result<int, IoError>` | Writes UTF-8 bytes of `text`. |
| `write_all(data)` | `Result<bool, IoError>` | Retries until every byte is written. |
| `read_exact(count)` | `Result<byte[], IoError>` | Reads exactly `count` bytes or fails with `EEOF`. |
| `seek(pos)` | `Result<bool, IoError>` | Moves to an absolute byte offset. |
| `seek_from_end(offset)` | `Result<bool, IoError>` | Moves relative to the end; negative moves backward. |
| `seek_end()` | `Result<bool, IoError>` | Moves to the end. |
| `tell()` | `Result<long, IoError>` | Current byte offset. |
| `length()` | `Result<long, IoError>` | Current file length in bytes. |
| `flush()` | `Result<bool, IoError>` | Checks the handle is open; writes are unbuffered. |
| `close()` | `Result<bool, IoError>` | Closes the handle. |
| `is_closed` | `bool` | Whether the handle is closed. |
| `path` | `string` | Path used to open the file. |

The async forms `read_async(n, token)`, `write_async(data, token)`,
`seek_async(pos, token)`, and `flush_async(token)` return the same results.
Neither `flush()` nor `flush_async()` forces data to durable storage.

Use `Streams.read_text(file)` to read the remaining bytes as UTF-8 text, or `file.read_line()`
for line-by-line input. Byte reads use `file.read(n)`. For chunked Unicode input use an
incremental decoder so a chunk boundary cannot split a character.

`close()` is idempotent. Operations after closure return `ECLOSED`. Dropping the
last reference also closes the handle, but only explicit closure can report an
error. A failed close retires the handle: retrying an OS descriptor could close a
different file after the descriptor has been reused.

File positions and lengths use `long`. `tell()` includes buffered read-ahead, so
mixing line and byte reads preserves the logical position. Failed seeks preserve
the position and any unread bytes. `FileHandle.tell()` and `.length()` return
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
    let source = FileHandle.open("input.bin", FileMode.Read)?;
    let destination = FileHandle.open("output.bin", FileMode.Write)?;
    Streams.copy(source, destination, buffer_size: 65536)?;
    destination.close()?;
    return source.close();
}
```

| Function | Returns | Effect |
|---|---|---|
| `Streams.copy(source, destination, buffer_size)` | `Result<long, IoError>` | Copies a `Reader` to a `Writer`. |
| `Streams.copy_async(source, destination, buffer_size, token)` | `async Result<long, IoError>` | Copies an `AsyncReader` to an `AsyncWriter`. |

Both copy to EOF in chunks of at most `buffer_size` bytes (default 8192), flush the
destination, and return the byte count. Neither closes either stream. A
nonpositive `buffer_size` returns `EINVAL`. Completed writes stay in place
after a failure or cancellation.

## Stream interfaces

Implement these interfaces to plug your own types into the copy helpers and
buffered wrappers:

| Interface | Members | Notes |
|---|---|---|
| `Reader` | `read(max_bytes)`, `seek(position)`, `tell()`, `read_exact(count)` | Only `read` is required; default `seek`/`tell` return `ENOTSUP`. |
| `Writer` | `write(data)`, `flush()`, `write_all(data)` | `write_all` is provided. |
| `AsyncReader` | `read_async(max_bytes, token)` | Same EOF and size rules as `Reader`. |
| `AsyncWriter` | `write_async(data, token)`, `flush_async(token)` | Writes may accept fewer bytes than given. |

`Reader.read_exact` reports `EEOF` if the stream ends early. `Writer.write_all`
retries short writes and reports an error if the writer makes no progress.
Interface implementations must return no more bytes than requested.

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

| Member | Returns | Effect |
|---|---|---|
| `BufferedReader(source, buffer_size)` | `BufferedReader` | Wraps a `Reader`; `buffer_size` defaults to 8192. |
| `BufferedReader.read(max_bytes)` | `Result<byte[], IoError>` | Reads from the buffer, refilling as needed. |
| `BufferedReader.read_line(max_bytes)` | `Result<Option<string>, IoError>` | Reads one line; `None` at EOF. |
| `BufferedReader.seek(position)` / `tell()` | `Result<bool, IoError>` / `Result<long, IoError>` | Repositions the source / reports the logical position. |
| `BufferedReader.take_buffered()` | `byte[]` | Removes and returns unread buffered bytes. |
| `BufferedWriter(destination, buffer_size)` | `BufferedWriter` | Wraps a `Writer`; `buffer_size` defaults to 8192. |
| `BufferedWriter.write(data)` | `Result<int, IoError>` | Buffers bytes; may accept only part. |
| `BufferedWriter.flush()` | `Result<bool, IoError>` | Writes pending bytes and flushes the destination. |
| `BufferedWriter.finish()` | `Result<bool, IoError>` | Flushes and rejects later writes with `ECLOSED`. |

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

| `MemoryStream` member | Returns | Effect |
|---|---|---|
| `MemoryStream()` | `MemoryStream` | Creates an empty stream. |
| `MemoryStream(bytes)` | `MemoryStream` | Copies `bytes` and starts at byte zero. |
| `read(max_bytes)` | `Result<byte[], IoError>` | Reads and advances the cursor. |
| `write(bytes)` | `Result<int, IoError>` | Overwrites or appends at the cursor. |
| `seek(position)` | `Result<bool, IoError>` | Moves the cursor, including past the end. |
| `tell()` | `Result<long, IoError>` | Current position; fails after closure. |
| `position` / `length` | `long` | Cursor and stored size; readable after closure. |
| `to_bytes()` | `Result<byte[], IoError>` | Copy of all stored bytes. |
| `flush()` | `Result<bool, IoError>` | Checks the stream is open. |
| `close()` | `Result<bool, IoError>` | Closes idempotently; data is retained. |

`MemoryStream` also implements `read_async`, `write_async`, and `flush_async`.
Positions are limited to the `int` range.

## Error codes

Stream operations fail with `IoError`, which has `code()`, `message()`, and a
`path` field that is empty when no file is involved.

| Code | Meaning |
|---|---|
| `ENOENT` | The file does not exist. |
| `EACCES` | Permission was denied. |
| `EEXIST` | The path already exists. |
| `EINVAL` | Invalid argument, such as a negative size or nonpositive buffer size. |
| `ECLOSED` | The stream was already closed or finished. |
| `EBUSY` | Another operation is pending on the handle, or the file executor is saturated. |
| `ECANCELLED` | A cancellation token fired. |
| `ENOTSUP` | The stream does not support seeking or positions. |
| `EEOF` | `read_exact` reached the end early. |
| `ELIMIT` | A line exceeded the `read_line` byte limit. |
| `EIO` | Any other I/O failure. |

## Asynchronous file copies

```dream
import system.io;
import system.encoding;

async fun main(): Result<bool, IoError> {
    let source = MemoryStream(Encoding.utf8_encode("hello"));
    let destination = FileHandle.open_async("async-copy.txt", FileMode.WriteRead).await?;
    Streams.copy_async(source, destination).await?;
    destination.seek_async(0L).await?;
    let bytes = destination.read_async(5).await?;
    destination.close()?;
    source.close()?;
    File.delete("async-copy.txt");
    return Result.Ok(bytes.length == 5);
}
```

## Standard input and output

Import `system.io` to use `System.stdin` as a `Reader` and `System.stdout` or
`System.stderr` as a `Writer`. Byte reads and writes report `IoError`; they leave the
process descriptors open. Standard streams do not support seeking.

```dream
import system;
import system.io;
import system.encoding;

fun main(): void {
    let writer = BufferedWriter(System.stdout);
    writer.write_all(Encoding.utf8_encode("Ready\n")).unwrap();
    writer.finish().unwrap();

    let reader = BufferedReader(System.stdin);
    switch (reader.read_line().unwrap()) {
        Some(line) => System.println(line),
        None => {},
    }
}
```

`Streams.copy(System.stdin, destination)` copies incoming bytes into a writer.
`Streams.copy(source, System.stdout)` copies from a reader to standard output;
flush the output when the copy finishes. Reading standard input can block.

`System.print` and `System.println` are console text shortcuts for writing to
standard output. They flush their output, preserving order when mixed with
`System.stdout.write`. `System.read_line` and `System.read_key` remain shortcuts
for simple console input. Use `BufferedReader` and `Encoding` for text over a byte stream.

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

Reads, writes, seeks, length, and `close` return `Result`. An empty successful read means EOF, and a failure returns `IoError`.

```dream
import system.io;

fun main(): Result<Unit, IoError> {
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
| `write(data)` | `Result<int, IoError>` | Writes bytes; returns the count written. Pass `Encoding.utf8_encode(text)` to write text. |
| `write_all(data)` | `Result<Unit, IoError>` | Retries until every byte is written. |
| `read_exact(count)` | `Result<byte[], IoError>` | Reads exactly `count` bytes or fails with `EEOF`. |
| `seek(pos)` | `Result<Unit, IoError>` | Moves to an absolute byte offset. |
| `seek_from_end(offset)` | `Result<Unit, IoError>` | Moves relative to the end; negative moves backward. |
| `tell()` | `Result<long, IoError>` | Current byte offset. |
| `size()` | `Result<long, IoError>` | Current file size in bytes. |
| `flush()` | `Result<Unit, IoError>` | Checks the handle is open; writes are unbuffered. |
| `close()` | `Result<Unit, IoError>` | Closes the handle. |
| `is_closed` | `bool` | Whether the handle is closed. |
| `path` | `string` | Path used to open the file. |

The async forms `read_async(n, token)`, `write_async(data, token)`,
`seek_async(pos, token)`, and `flush_async(token)` return the same results.
Neither `flush()` nor `flush_async()` forces data to durable storage.

`Streams.read_text(file)` reads the rest as UTF-8, `read_line()` reads one line, and `read(n)` reads bytes. Use an incremental decoder when a chunk might split a character.

`close()` is safe to call again, and later operations return `ECLOSED`. Dropping the last reference also closes the handle, but only `close()` can report an error.

A failed close retires the handle. Do not retry in a way that could close a different file after the descriptor is reused.

Positions and sizes are `long`. `tell()` includes buffered read-ahead, so mixing line and byte reads keeps the logical position, and a failed seek keeps that position and any unread bytes.

Async file calls run on a native pool of four workers, with at most 128 queued jobs. A full queue returns `EBUSY`, and browser memory-file operations are ready immediately.

A handle allows one pending operation. A second I/O call or `close` returns `EBUSY` and leaves the handle open.

After cancellation, wait for the work already started to finish before you reuse or close that handle. Dropping the handle while that work continues is safe.

Cancellation does not undo a completed write or put consumed bytes back. A cancelled open closes a late result, and a seek already submitted finishes and updates read-ahead before cancellation is reported.

## Streaming copies

```dream
import system.io;

fun main(): Result<Unit, IoError> {
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

Both copy until EOF in chunks of at most `buffer_size` bytes (default 8192), flush the destination, and return the byte count. Neither closes either stream.

A nonpositive `buffer_size` returns `EINVAL`. Bytes already written stay written after a failure or cancellation.

## Stream interfaces

Implement these interfaces to plug your own types into the copy helpers and
buffered wrappers:

| Interface | Members | Notes |
|---|---|---|
| `Reader` | `read(max_bytes)`, `seek(position)`, `tell()`, `read_exact(count)` | Only `read` is required; default `seek`/`tell` return `ENOTSUP`. |
| `Writer` | `write(data)`, `flush()`, `write_all(data)` | `write_all` is provided. |
| `AsyncReader` | `read_async(max_bytes, token)` | Same EOF and size rules as `Reader`. |
| `AsyncWriter` | `write_async(data, token)`, `flush_async(token)` | Writes may accept fewer bytes than given. |

`read_exact` returns `EEOF` if the stream ends early. `write_all` retries a short write and errors if a write makes no progress; do not return more bytes than requested.

## Buffered and memory streams

```dream
import system.io;
import system.encoding;

fun main(): Result<Unit, IoError> {
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
| `BufferedReader.seek(position)` / `tell()` | `Result<Unit, IoError>` / `Result<long, IoError>` | Repositions the source / reports the logical position. |
| `BufferedReader.take_buffered()` | `byte[]` | Removes and returns unread buffered bytes. |
| `BufferedWriter(destination, buffer_size)` | `BufferedWriter` | Wraps a `Writer`; `buffer_size` defaults to 8192. |
| `BufferedWriter.write(data)` | `Result<int, IoError>` | Buffers bytes; may accept only part. |
| `BufferedWriter.flush()` | `Result<Unit, IoError>` | Writes pending bytes and flushes the destination. |
| `BufferedWriter.finish()` | `Result<Unit, IoError>` | Flushes and rejects later writes with `ECLOSED`. |

A buffer wrapper keeps the inner stream open. Call `finish()` so a flush error is visible; dropping the writer does not flush.

After a partial flush failure, unwritten bytes stay available to retry. A successful `finish()` rejects later writes with `ECLOSED`.

Seek through the buffered reader, and do not touch the inner source while the wrapper holds unread bytes. `take_buffered()` returns those bytes.

Line reads accept LF and CRLF, keep a standalone carriage return, and return a final line with no newline. The default limit is 1 MiB (`read_line(max_bytes: ...)`).

A limit error consumes the prefix and the byte past the limit. The buffer size must be positive; an invalid size returns `EINVAL` when you use it.

A memory stream can seek, overwrite, and fill gaps with zeros. `to_bytes()` returns a copy, and its async methods are ready immediately.

| `MemoryStream` member | Returns | Effect |
|---|---|---|
| `MemoryStream()` | `MemoryStream` | Creates an empty stream. |
| `MemoryStream(bytes)` | `MemoryStream` | Copies `bytes` and starts at byte zero. |
| `read(max_bytes)` | `Result<byte[], IoError>` | Reads and advances the cursor. |
| `write(bytes)` | `Result<int, IoError>` | Overwrites or appends at the cursor. |
| `seek(position)` | `Result<Unit, IoError>` | Moves the cursor, including past the end. |
| `tell()` | `Result<long, IoError>` | Current position; fails after closure. |
| `position` / `length` | `long` | Cursor and stored size; readable after closure. |
| `to_bytes()` | `Result<byte[], IoError>` | Copy of all stored bytes. |
| `flush()` | `Result<Unit, IoError>` | Checks the stream is open. |
| `close()` | `Result<Unit, IoError>` | Closes idempotently; data is retained. |

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

async fun main(): Result<Unit, IoError> {
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

With `system.io`, `System.stdin` is a `Reader` and `System.stdout` and `System.stderr` are writers. Reads and writes return `IoError`, leave the process streams open, and cannot seek.

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

`Streams.copy` connects standard input or output to another stream. Flush standard output when a copy to it finishes; reading standard input can block.

`System.print` and `System.println` write text and flush, so they stay in order with `System.stdout.write`.

`System.read_line` and `System.read_key` are the shortcuts for simple console input. Use `BufferedReader` and `Encoding` for text over a byte stream.

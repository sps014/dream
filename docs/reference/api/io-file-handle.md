# File Handle

**Import:** `import system.io;`

## `FileHandle`

FileHandle - an OS-backed read/write/seek stream over an open file. Async operations use bounded native workers or Node callbacks, keeping host buffers alive through cancellation. Concurrent I/O on one handle returns `EBUSY`; closure is synchronous.

```dream
public class FileHandle: Reader, Writer, AsyncReader, AsyncWriter
```

## `is_closed`

Whether this handle has been closed, including after an explicit close failure.

```dream
public get is_closed(): bool
```

## `path`

Path supplied when opening the file; remains available after closure.

```dream
public get path(): string
```

## `open`

Opens a file with the selected access, creation, and truncation behavior.

```dream
public static fun open(path: string, mode: FileMode): Result<FileHandle, IoError>
```

## `open_async`

Opens using host async facilities or the bounded filesystem executor. Cancellation abandons the result safely; a late open result is closed automatically.

```dream
public static async fun open_async(
    path: string,
    mode: FileMode,
    token: Option<CancellationToken> = Option.None
): Result<FileHandle, IoError>
```

## `read`

Reads up to `n` bytes from the current position, advancing the cursor.

```dream
public fun read(n: int): Result<byte[], IoError>
```

## `read_async`

Reads without blocking the scheduler, preserving partial reads and EOF. Cancellation may consume input; further I/O returns `EBUSY` until host work ends.

```dream
public async unique fun read_async(
    n: int,
    token: Option<CancellationToken> = Option.None
): Result<byte[], IoError>
```

## `write`

Writes `data` at the current position, advancing the cursor; returns bytes written.

```dream
public fun write(data: byte[]): Result<int, IoError>
```

## `write_async`

Writes a host-owned copy without blocking the scheduler and returns partial progress. Cancellation does not roll back writes; further I/O is busy until host work ends.

```dream
public async unique fun write_async(
    data: byte[],
    token: Option<CancellationToken> = Option.None
): Result<int, IoError>
```

## `seek`

Seeks to absolute byte offset `pos` from the start of the file.

```dream
public fun seek(pos: long): Result<bool, IoError>
```

## `seek_async`

Seeks on the filesystem executor; successful completion discards read-ahead. Once submitted, cancellation is observed after completion so buffer state stays coherent.

```dream
public async unique fun seek_async(
    pos: long,
    token: Option<CancellationToken> = Option.None
): Result<bool, IoError>
```

## `tell`

Current cursor offset from the start of the file.

```dream
public fun tell(): Result<long, IoError>
```

## `seek_from_end`

Seeks to `offset` bytes from the end (`0` is the end; negative moves backward).

```dream
public fun seek_from_end(offset: long): Result<bool, IoError>
```

## `seek_end`

Moves the cursor to the end of the file.

```dream
public fun seek_end(): Result<bool, IoError>
```

## `read_line`

Reads a line using buffered read-ahead, stripping LF or CRLF and preserving standalone CR. Returns the final unterminated line or `None` at EOF; `ELIMIT` consumes the prefix and exceeding byte beyond the pre-LF byte limit (default 1 MiB, including any CR).

```dream
public fun read_line(max_bytes: int = 1048576): Result<Option<string>, IoError>
```

## `write_text`

Writes UTF-8 bytes of `text` at the current position.

```dream
public fun write_text(text: string): Result<int, IoError>
```

## `length`

Returns the current byte length from the open handle; propagates metadata failures.

```dream
public fun length(): Result<long, IoError>
```

## `flush`

Validates that the handle is open; writes are unbuffered at this layer. This does not synchronize data to durable storage.

```dream
public unique fun flush(): Result<bool, IoError>
```

## `flush_async`

Checks cancellation and validates the handle; this layer has no buffered output to flush.

```dream
public async unique fun flush_async(
    token: Option<CancellationToken> = Option.None
): Result<bool, IoError>
```

## `close`

Closes idempotently and reports host failures. The handle stays closed even on failure to prevent closing a reused descriptor; destruction performs nonthrowing cleanup.

```dream
public fun close(): Result<bool, IoError>
```

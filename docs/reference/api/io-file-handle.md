# File Handle

**Import:** `import system.io;`

## `public class FileHandle : Reader, Writer`

FileHandle - an OS-backed read/write/seek stream over an open file. Host calls are synchronous; `*_async` methods wrap the same operations in a `Future` for await-style call sites. `close` is sync-only.

```dream
public class FileHandle : Reader, Writer
```

## `public get is_closed`

Whether this handle has been closed, including after an explicit close failure.

```dream
public get is_closed(): bool
```

## `public get path`

Path supplied when opening the file; remains available after closure.

```dream
public get path(): string
```

## `public static fun open`

Opens `path` with `mode` ("r", "w", "a", "r+", "w+", "a+").

```dream
public static fun open(path: string, mode: string): Result<FileHandle, IoError>
```

## `public static async fun open_async`

Checks cancellation, then runs synchronous `open` inside an awaitable operation. Pending host work is not interruptible and may block the scheduler.

```dream
public static async fun open_async(path: string, mode: string, token: Option<CancellationToken> = Option.None): Result<FileHandle, IoError>
```

## `public fun read`

Reads up to `n` bytes from the current position, advancing the cursor.

```dream
public fun read(n: int): Result<byte[], IoError>
```

## `public async fun read_async`

Checks cancellation, then runs synchronous `read` inside an awaitable operation. Pending host work is not interruptible and may block the scheduler.

```dream
public async fun read_async(n: int, token: Option<CancellationToken> = Option.None): Result<byte[], IoError>
```

## `public fun write`

Writes `data` at the current position, advancing the cursor; returns bytes written.

```dream
public fun write(data: byte[]): Result<int, IoError>
```

## `public async fun write_async`

Checks cancellation, then runs synchronous `write` inside an awaitable operation. Pending host work is not interruptible and may block the scheduler.

```dream
public async fun write_async(data: byte[], token: Option<CancellationToken> = Option.None): Result<int, IoError>
```

## `public fun seek`

Seeks to absolute byte offset `pos` from the start of the file.

```dream
public fun seek(pos: long): Result<bool, IoError>
```

## `public async fun seek_async`

Checks cancellation, then runs synchronous `seek` inside an awaitable operation. Pending host work is not interruptible and may block the scheduler.

```dream
public async fun seek_async(pos: long, token: Option<CancellationToken> = Option.None): Result<bool, IoError>
```

## `public fun tell`

Current cursor offset from the start of the file.

```dream
public fun tell(): Result<long, IoError>
```

## `public fun seek_from_end`

Seeks to `offset` bytes from the end (`0` is the end; negative moves backward).

```dream
public fun seek_from_end(offset: long): Result<bool, IoError>
```

## `public fun seek_end`

Moves the cursor to the end of the file.

```dream
public fun seek_end(): Result<bool, IoError>
```

## `public fun read_line`

Reads a line using buffered read-ahead, stripping LF or CRLF and preserving standalone CR. Returns the final unterminated line or `None` at EOF; `ELIMIT` consumes the prefix and exceeding byte beyond the pre-LF byte limit (default 1 MiB, including any CR).

```dream
public fun read_line(max_bytes: int = 1048576): Result<Option<string>, IoError>
```

## `public fun write_text`

Writes UTF-8 bytes of `text` at the current position.

```dream
public fun write_text(text: string): Result<int, IoError>
```

## `public fun length`

Returns the current byte length from the open handle; propagates metadata failures.

```dream
public fun length(): Result<long, IoError>
```

## `public unique fun flush`

Validates that the handle is open; writes are unbuffered at this layer. This does not synchronize data to durable storage.

```dream
public unique fun flush(): Result<bool, IoError>
```

## `public fun close`

Closes idempotently and reports host failures. The handle stays closed even on failure to prevent closing a reused descriptor; destruction performs nonthrowing cleanup.

```dream
public fun close(): Result<bool, IoError>
```

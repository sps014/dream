# FileHandle

**Import:** `import system.io;`

Read the [usage guide](../stdlib/file.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class FileHandle`

FileHandle - an OS-backed read/write/seek stream over an open file. Host calls are synchronous; `*_async` methods wrap the same operations in a `Future` for await-style call sites. `close` is sync-only.

```dream
public class FileHandle
```

## `path`

```dream
public get path(): string
```

## `open`

Opens `path` with `mode` ("r", "w", "a", "r+", "w+", "a+").

```dream
public static fun open(path: string, mode: string): Result<FileHandle, IoError>
```

## `open_async`

Async wrapper around `open`.

```dream
public static async fun open_async(path: string, mode: string, token: Option<CancellationToken> = Option.None): Result<FileHandle, IoError>
```

## `read`

Reads up to `n` bytes from the current position, advancing the cursor.

```dream
public fun read(n: int): Result<byte[], IoError>
```

## `read_async`

Async wrapper around `read`.

```dream
public async fun read_async(n: int, token: Option<CancellationToken> = Option.None): Result<byte[], IoError>
```

## `write`

Writes `data` at the current position, advancing the cursor; returns bytes written.

```dream
public fun write(data: byte[]): Result<int, IoError>
```

## `write_async`

Async wrapper around `write`.

```dream
public async fun write_async(data: byte[], token: Option<CancellationToken> = Option.None): Result<int, IoError>
```

## `seek`

Seeks to absolute byte offset `pos` from the start of the file.

```dream
public fun seek(pos: long): Result<bool, IoError>
```

## `seek_async`

Async wrapper around `seek`.

```dream
public async fun seek_async(pos: long, token: Option<CancellationToken> = Option.None): Result<bool, IoError>
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

Reads bytes until `\n` (not included) or EOF. `None` at EOF with no leftover bytes.

```dream
public fun read_line(): Result<Option<string>, IoError>
```

## `write_text`

Writes UTF-8 bytes of `text` at the current position.

```dream
public fun write_text(text: string): Result<int, IoError>
```

## `close`

Releases the OS handle (sync only).

```dream
public fun close(): void
```

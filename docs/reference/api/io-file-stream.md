# FileStream

**Import:** `import system.io;`

Read the [usage guide](../stdlib/file.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class FileStream`

FileStream - a seekable cursor over an open `FileHandle` (no whole-file preload).

```dream
public class FileStream
```

## `read_bytes`

Reads up to `n` raw bytes from the current position, advancing the cursor.

```dream
public fun read_bytes(n: int): byte[]
```

## `read`

Reads up to `n` bytes from the current position as UTF-8 text, advancing the cursor.

```dream
public fun read(n: int): string
```

## `read_all`

Reads everything remaining from the current position as text.

```dream
public fun read_all(): string
```

## `read_line`

Reads one line (without the trailing newline), or `None` at EOF.

```dream
public fun read_line(): Option<string>
```

## `write_text`

```dream
public fun write_text(text: string): Result<int, IoError>
```

## `has_more`

True while the cursor has not reached the end of the file.

```dream
public fun has_more(): bool
```

## `position`

Current cursor offset (bytes from the start).

```dream
public get position(): int
```

## `position`

```dream
public set position(offset: int)
```

## `length`

Total file size in bytes (`-1` when the path is missing).

```dream
public get length(): int
```

## `seek`

Moves the cursor to an absolute offset from the start of the file.

```dream
public fun seek(offset: int): void
```

## `seek_end`

Moves the cursor to the end of the file.

```dream
public fun seek_end(): void
```

## `tell`

Current cursor as a `long` (same as `position`, for `FileHandle.tell` parity).

```dream
public fun tell(): long
```

## `reset`

Rewinds the cursor to the start.

```dream
public fun reset(): void
```

## `close`

Releases the underlying OS handle.

```dream
public fun close(): void
```

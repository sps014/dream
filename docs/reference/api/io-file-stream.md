# FileStream

**Import:** `import system.io;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `FileStream`

Text and byte access over a file handle, with propagated I/O errors and byte positions.

```dream
public class FileStream
```

## `read_bytes`

Reads up to `n` bytes, advancing the file position; empty success indicates EOF for positive sizes, and negative sizes return an error.

```dream
public fun read_bytes(n: int): Result<byte[], IoError>
```

## `read`

Reads up to `n` bytes and decodes them as UTF-8; the limit and cursor are byte-based. A read can split a UTF-8 sequence; use an incremental decoder for streamed text.

```dream
public fun read(n: int): Result<string, IoError>
```

## `read_all`

Collects all remaining bytes through EOF and decodes them as UTF-8; propagates read errors. Memory use grows with the remaining file size.

```dream
public fun read_all(): Result<string, IoError>
```

## `read_line`

Reads an LF or CRLF terminated line without its terminator, or a final unterminated line. Returns `None` at EOF; propagates I/O errors and the handle default line-size limit.

```dream
public fun read_line(): Result<Option<string>, IoError>
```

## `write_text`

Writes UTF-8 bytes at the current position and returns the byte count; may write partially.

```dream
public fun write_text(text: string): Result<int, IoError>
```

## `has_more`

Compares the current position with the current file length, propagating errors. Concurrent file changes can invalidate the result before the next read.

```dream
public fun has_more(): Result<bool, IoError>
```

## `length`

Returns the current file length in bytes, or an I/O or closed-resource error.

```dream
public get length(): Result<long, IoError>
```

## `seek`

Seeks to an absolute byte offset; failure preserves the previous logical position.

```dream
public fun seek(offset: long): Result<bool, IoError>
```

## `seek_end`

Moves to the current end of the file; propagates positioning errors.

```dream
public fun seek_end(): Result<bool, IoError>
```

## `tell`

Returns the current byte offset, accounting for buffered read-ahead.

```dream
public fun tell(): Result<long, IoError>
```

## `close`

Closes the underlying handle idempotently and reports explicit closure failures.

```dream
public fun close(): Result<bool, IoError>
```

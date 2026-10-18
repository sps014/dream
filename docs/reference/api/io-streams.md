# Streams

**Import:** `import system.io;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Streams`

Copy helpers that leave ownership and closure of both streams with the caller.

```dream
public static class Streams
```

## `read_text`

Reads all remaining bytes and decodes them as UTF-8. Memory grows with the input.

```dream
public static fun read_text(source: Reader): Result<string, IoError>
```

## `copy`

Copies to EOF using chunks bounded by positive `buffer_size` (default 8 KiB), then flushes the destination and returns the byte count; neither stream is closed.

```dream
public static fun copy(
    source: Reader,
    destination: Writer,
    buffer_size: int = 8192
): Result<long, IoError>
```

## `copy_async`

Copies to EOF with bounded chunks, partial-write retries, and cancellation checks, then flushes and returns the byte count. Neither stream is closed; completed writes persist.

```dream
public static async fun copy_async(
    source: AsyncReader,
    destination: AsyncWriter,
    buffer_size: int = 8192,
    token: Option<CancellationToken> = Option.None
): Result<long, IoError>
```

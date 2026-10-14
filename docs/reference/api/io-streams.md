# Streams

**Import:** `import system.io;`

## `public interface Reader`

Byte input with partial reads and optional positioning; operations report I/O failures.

```dream
public interface Reader
```

## `unique fun read`

Reads at most `max_bytes` bytes; empty success means EOF for a positive size. A zero size consumes nothing; negative sizes and closed resources return errors.

```dream
unique fun read(max_bytes: int): Result<byte[], IoError>
```

## `public unique fun seek`

Moves to an absolute byte position; unsupported readers return `ENOTSUP`. A failed seek preserves the previous logical position.

```dream
public unique fun seek(position: long): Result<bool, IoError>
```

## `public fun tell`

Returns the logical byte position, or `ENOTSUP` when positioning is unavailable.

```dream
public fun tell(): Result<long, IoError>
```

## `public fun read_exact`

Collects exactly `count` bytes, retrying short reads; premature EOF returns `EEOF`. Errors may occur after input has already been consumed; negative counts are invalid.

```dream
public fun read_exact(count: int): Result<byte[], IoError>
```

## `public interface Writer`

Byte output with partial writes and explicit flushing.

```dream
public interface Writer
```

## `unique fun write`

Accepts up to the supplied bytes and returns the number accepted. Empty input makes no progress; failures do not undo bytes already written.

```dream
unique fun write(data: byte[]): Result<int, IoError>
```

## `unique fun flush`

Delivers buffered output to the underlying destination and reports failures. Flushing does not imply durable storage or close the destination.

```dream
unique fun flush(): Result<bool, IoError>
```

## `public fun write_all`

Retries partial writes until all bytes are accepted; rejects zero or invalid progress. A failure leaves any completed writes in effect; this does not flush.

```dream
public fun write_all(data: byte[]): Result<bool, IoError>
```

## `public interface AsyncReader`

Awaitable byte input with the same EOF and size rules as `Reader`.

```dream
public interface AsyncReader
```

## `async unique fun read_async`

Reads up to `max_bytes` with cooperative cancellation. Cancellation does not restore bytes already consumed by the underlying operation.

```dream
async unique fun read_async(max_bytes: int, token: Option<CancellationToken> = Option.None): Result<byte[], IoError>
```

## `public interface AsyncWriter`

Awaitable byte output with explicit flushing and cooperative cancellation.

```dream
public interface AsyncWriter
```

## `async unique fun write_async`

Returns the number of bytes accepted, which may be less than the input length. Cancellation or failure does not roll back completed writes.

```dream
async unique fun write_async(data: byte[], token: Option<CancellationToken> = Option.None): Result<int, IoError>
```

## `async unique fun flush_async`

Flushes pending output with cancellation; does not close the destination or guarantee durable storage.

```dream
async unique fun flush_async(token: Option<CancellationToken> = Option.None): Result<bool, IoError>
```

## `public static class Streams`

Copy helpers that leave ownership and closure of both streams with the caller.

```dream
public static class Streams
```

## `public static fun copy`

Copies to EOF using chunks bounded by positive `buffer_size` (default 8 KiB), then flushes the destination and returns the byte count; neither stream is closed.

```dream
public static fun copy(source: Reader, destination: Writer, buffer_size: int = 8192): Result<long, IoError>
```

## `public static async fun copy_async`

Copies to EOF with bounded chunks, partial-write retries, and cancellation checks, then flushes and returns the byte count. Neither stream is closed; completed writes persist.

```dream
public static async fun copy_async(source: AsyncReader, destination: AsyncWriter, buffer_size: int = 8192, token: Option<CancellationToken> = Option.None): Result<long, IoError>
```

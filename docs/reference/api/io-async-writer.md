# AsyncWriter

**Import:** `import system.io;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `AsyncWriter`

Awaitable byte output with explicit flushing and cooperative cancellation.

```dream
public interface AsyncWriter
```

## `write_async`

Returns the number of bytes accepted, which may be less than the input length. Cancellation or failure does not roll back completed writes.

```dream
async unique fun write_async(
    data: byte[],
    token: Option<CancellationToken> = Option.None
): Result<int, IoError>
```

## `flush_async`

Flushes pending output with cancellation; does not close the destination or guarantee durable storage.

```dream
async unique fun flush_async(
    token: Option<CancellationToken> = Option.None
): Result<Unit, IoError>
```

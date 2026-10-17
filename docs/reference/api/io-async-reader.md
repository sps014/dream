# AsyncReader

**Import:** `import system.io;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `AsyncReader`

Awaitable byte input with the same EOF and size rules as `Reader`.

```dream
public interface AsyncReader
```

## `read_async`

Reads up to `max_bytes` with cooperative cancellation. Cancellation does not restore bytes already consumed by the underlying operation.

```dream
async unique fun read_async(
    max_bytes: int,
    token: Option<CancellationToken> = Option.None
): Result<byte[], IoError>
```

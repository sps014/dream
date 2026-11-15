# HttpBody

**Import:** `import system.net;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `HttpBody`

An HTTP response byte stream. One read may be pending; cancellation can consume bytes.

```dream
public class HttpBody: AsyncReader
```

## `read_async`

Reads up to `max_bytes`; empty success for a positive request is EOF. Zero consumes nothing. Errors use `IoError` for compatibility with async streams; concurrent reads return `EBUSY`.

```dream
public async unique fun read_async(
    max_bytes: int,
    token: Option<CancellationToken> = Option.None
): Result<byte[], IoError>
```

## `close`

Releases the response transport and cancels a pending read; safe to call repeatedly.

```dream
public fun close(): Result<Unit, HttpError>
```

# CompressionReader

**Import:** `import system.compress;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `CompressionReader`

Incremental decompressor with bounded chunks, checksum validation, and strict trailing-byte rejection.

```dream
public class CompressionReader: Reader
```

## `read`

Returns up to `max_bytes` (at most 64 KiB) decoded bytes, or an empty array once the stream is complete. Malformed, truncated, oversized, or concatenated input returns an `ECOMPRESS` error, and source read failures are returned unchanged; either closes the reader.

```dream
public fun read(max_bytes: int): Result<byte[], IoError>
```

## `close`

Stops decoding without closing the borrowed source; later reads fail.

```dream
public fun close(): void
```

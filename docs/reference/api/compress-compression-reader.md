# CompressionReader

**Import:** `import system.compress;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `CompressionReader`

Incremental decompressor with bounded chunks, checksum validation, and strict trailing-byte rejection.

```dream
public class CompressionReader: Reader
```

## `read`

Returns partial decoded bytes; malformed, truncated, oversized, or concatenated input returns an error.

```dream
public fun read(max_bytes: int): Result<byte[], IoError>
```

## `close`

Releases codec state without closing the borrowed source.

```dream
public fun close(): void
```

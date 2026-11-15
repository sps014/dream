# Compression

**Import:** `import system.compress;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Compression`

Streaming gzip/zlib factories; wrappers borrow their underlying stream.

```dream
public static class Compression
```

## `writer`

Creates an encoder; max_output_bytes bounds compressed output across every write and finish.

```dream
public static fun writer(
    destination: Writer,
    format: CompressionFormat = CompressionFormat.Gzip,
    max_output_bytes: long = 67108864L
): Result<CompressionWriter, IoError>
```

## `reader`

Creates a strict single-stream decoder; the output limit prevents unbounded expansion.

```dream
public static fun reader(
    source: Reader,
    format: CompressionFormat = CompressionFormat.Gzip,
    max_output_bytes: long = 67108864L
): Result<CompressionReader, IoError>
```

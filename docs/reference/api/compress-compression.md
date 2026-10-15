# Compression

**Import:** `import system.compress;`

## `CompressionFormat`

Standard wrapper format for a single compressed stream.

```dream
public enum CompressionFormat
```

## `Gzip`

Gzip framing with CRC and uncompressed-size validation.

```dream
Gzip
```

## `Zlib`

Zlib framing with Adler-32 validation.

```dream
Zlib
```

## `Compression`

Streaming gzip/zlib factories; wrappers borrow their underlying stream.

```dream
public static class Compression
```

## `writer`

Creates an encoder; max_output_bytes bounds compressed output across every write and finish.

```dream
public static fun writer(destination: Writer, format: CompressionFormat = CompressionFormat.Gzip, max_output_bytes: long = 67108864L): Result<CompressionWriter, IoError>
```

## `reader`

Creates a strict single-stream decoder; the output limit prevents unbounded expansion.

```dream
public static fun reader(source: Reader, format: CompressionFormat = CompressionFormat.Gzip, max_output_bytes: long = 67108864L): Result<CompressionReader, IoError>
```

## `CompressionWriter`

Incremental compressor over a borrowed Writer; finish explicitly to write and validate the trailer.

```dream
public class CompressionWriter : Writer
```

## `write`

Accepts up to 64 KiB; completed compressed writes remain visible if a later operation fails.

```dream
public fun write(bytes: byte[]): Result<int, IoError>
```

## `flush`

Emits a synchronization boundary and flushes the borrowed destination without ending the stream.

```dream
public fun flush(): Result<bool, IoError>
```

## `finish`

Finishes the trailer, releases encoder state, and flushes the borrowed destination.

```dream
public fun finish(): Result<bool, IoError>
```

## `close`

Abandons codec state without finishing or closing the borrowed destination.

```dream
public fun close(): void
```

## `CompressionReader`

Incremental decompressor with bounded chunks, checksum validation, and strict trailing-byte rejection.

```dream
public class CompressionReader : Reader
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

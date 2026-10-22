# CompressionWriter

**Import:** `import system.compress;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `CompressionWriter`

Incremental compressor over a borrowed Writer; finish explicitly to write and validate the trailer.

```dream
public class CompressionWriter: Writer
```

## `write`

Accepts up to 64 KiB and returns how many bytes were taken. Completed compressed output is written to the destination as blocks fill; exceeding the output limit returns an `ECOMPRESS` error and closes the writer.

```dream
public fun write(bytes: byte[]): Result<int, IoError>
```

## `flush`

Emits a synchronization boundary and flushes the borrowed destination without ending the stream.

```dream
public fun flush(): Result<Unit, IoError>
```

## `finish`

Finishes the trailer, releases encoder state, and flushes the borrowed destination.

```dream
public fun finish(): Result<Unit, IoError>
```

## `close`

Abandons the stream without finishing or closing the borrowed destination; later writes fail.

```dream
public fun close(): void
```

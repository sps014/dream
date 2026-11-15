# CompressionFormat

**Import:** `import system.compress;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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

# Stdin

**Import:** `import system.io;`

Read the [usage guide](../../library/io/streams.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Stdin`

Standard input as a byte stream. Obtain it through `System.stdin`.

```dream
public class Stdin: Reader
```

## `read`

Reads up to `max_bytes` bytes; an empty result indicates EOF for a positive size. Negative sizes return an error. This operation can block waiting for input.

```dream
public unique fun read(max_bytes: int): Result<byte[], IoError>
```

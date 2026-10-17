# Stderr

**Import:** `import system.io;`

Read the [usage guide](../../library/io/streams.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Stderr`

Standard error as a byte stream. Obtain it through `System.stderr`.

```dream
public class Stderr: Writer
```

## `write`

Writes bytes and returns the number accepted; failures leave completed writes in effect. Call `flush` to deliver any buffered bytes.

```dream
public unique fun write(data: byte[]): Result<int, IoError>
```

## `flush`

Delivers buffered output; returns an I/O error when flushing fails.

```dream
public unique fun flush(): Result<bool, IoError>
```

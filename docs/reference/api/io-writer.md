# Writer

**Import:** `import system.io;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Writer`

Byte output with partial writes and explicit flushing.

```dream
public interface Writer
```

## `write`

Accepts up to the supplied bytes and returns the number accepted. Empty input makes no progress; failures do not undo bytes already written.

```dream
unique fun write(data: byte[]): Result<int, IoError>
```

## `flush`

Delivers buffered output to the underlying destination and reports failures. Flushing does not imply durable storage or close the destination.

```dream
unique fun flush(): Result<Unit, IoError>
```

## `write_all`

Retries partial writes until all bytes are accepted; rejects zero or invalid progress. A failure leaves any completed writes in effect; this does not flush.

```dream
public fun write_all(data: byte[]): Result<Unit, IoError>
```

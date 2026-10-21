# Buffered Writer

**Import:** `import system.io;`

## `BufferedWriter`

Buffers output to a borrowed writer without closing it. Call `finish()` explicitly to report flush failures; destruction does not flush pending bytes.

```dream
public class BufferedWriter: Writer
```

## `constructor`

Wraps `destination` with an 8 KiB default buffer; nonpositive sizes fail on use.

```dream
public constructor(destination: Writer, buffer_size: int = 8192)
```

## `write`

Accepts as many bytes as fit, flushing a previously full buffer first. May return partial progress; operations after successful finish return `ECLOSED`.

```dream
public fun write(data: byte[]): Result<int, IoError>
```

## `flush`

Writes pending bytes and flushes the destination; retains any unwritten suffix on failure. Already completed writes persist, and a finished writer returns `ECLOSED`.

```dream
public fun flush(): Result<Unit, IoError>
```

## `finish`

Flushes and marks the wrapper finished only on success; repeated success is idempotent. Leaves the underlying writer open and permits retry after a flush failure.

```dream
public fun finish(): Result<Unit, IoError>
```

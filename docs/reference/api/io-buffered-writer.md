# Buffered Writer

**Import:** `import system.io;`

## `public class BufferedWriter : Writer`

Buffers output to a borrowed writer without closing it. Call `finish()` explicitly to report flush failures; destruction does not flush pending bytes.

```dream
public class BufferedWriter : Writer
```

## `public constructor`

Wraps `destination` with an 8 KiB default buffer; nonpositive sizes fail on use.

```dream
public constructor(destination: Writer, buffer_size: int = 8192)
```

## `public fun write`

Accepts as many bytes as fit, flushing a previously full buffer first. May return partial progress; operations after successful finish return `ECLOSED`.

```dream
public fun write(data: byte[]): Result<int, IoError>
```

## `public fun flush`

Writes pending bytes and flushes the destination; retains any unwritten suffix on failure. Already completed writes persist, and a finished writer returns `ECLOSED`.

```dream
public fun flush(): Result<bool, IoError>
```

## `public fun finish`

Flushes and marks the wrapper finished only on success; repeated success is idempotent. Leaves the underlying writer open and permits retry after a flush failure.

```dream
public fun finish(): Result<bool, IoError>
```

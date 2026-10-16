# Process Pipe

**Import:** `import system.process;`

Read the [process guide](../stdlib/process.md) for launch settings, cancellation, and streaming ownership.

## `ProcessReader`

A borrowed child output pipe; cancellation preserves bytes already consumed by the host.

```dream
public class ProcessReader: AsyncReader
```

## `read_async`

Reads at most max_bytes; a successful empty read for a positive size means EOF.

```dream
public async unique fun read_async(
    max_bytes: int,
    token: Option<CancellationToken> = Option.None
): Result<byte[], IoError>
```

## `read_line`

Reads a buffered UTF-8 line, removing LF or CRLF and preserving a final unterminated line.

```dream
public unique fun read_line(
    max_bytes: int = 1048576,
    token: Option<CancellationToken> = Option.None
): Future<Result<Option<string>, IoError>>
```

## `ProcessWriter`

A borrowed child input pipe implementing partial async writes.

```dream
public class ProcessWriter: AsyncWriter
```

## `write_async`

Writes bytes with partial progress; closing stdin delivers EOF to the child.

```dream
public async unique fun write_async(
    data: byte[],
    token: Option<CancellationToken> = Option.None
): Result<int, IoError>
```

## `flush_async`

Confirms cancellation and ownership; the process pipe has no guest-side output buffer.

```dream
public async unique fun flush_async(
    token: Option<CancellationToken> = Option.None
): Result<bool, IoError>
```

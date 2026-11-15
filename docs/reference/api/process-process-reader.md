# ProcessReader

**Import:** `import system.process;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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

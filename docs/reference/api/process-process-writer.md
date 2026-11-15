# ProcessWriter

**Import:** `import system.process;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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
): Result<Unit, IoError>
```

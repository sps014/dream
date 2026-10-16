# Child Process

**Import:** `import system.process;`

Read the [process guide](../stdlib/process.md) for launch settings, cancellation, and streaming ownership.

## `ChildProcess`

Owns a child and its pipes; explicit close terminates and reaps unfinished work.

```dream
public class ChildProcess
```

## `stdout`

Borrows stdout through an AsyncReader; retain this reader when using read_line buffering.

```dream
public get stdout(): ProcessReader
```

## `stderr`

Borrows stderr through an AsyncReader; drain it concurrently with stdout.

```dream
public get stderr(): ProcessReader
```

## `stdin`

Borrows stdin through an AsyncWriter; call close_stdin to deliver EOF to the child.

```dream
public get stdin(): ProcessWriter
```

## `close_stdin`

Closes stdin after pending writes, allowing commands waiting for input EOF to finish.

```dream
public fun close_stdin(
    token: Option<CancellationToken> = Option.None
): Future<Result<bool, ProcessError>>
```

## `wait`

Waits for process exit without discarding unread output or closing the child handle.

```dream
public fun wait(token: Option<CancellationToken> = Option.None): Future<Result<int, ProcessError>>
```

## `close`

Terminates and reaps the owned child; repeated or concurrent closes share the outcome.

```dream
public fun close(): Future<Result<bool, ProcessError>>
```

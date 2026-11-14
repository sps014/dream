# Child Process

**Import:** `import system.process;`

Read the [process guide](../../library/system/process.md) for launch settings, cancellation, and streaming ownership.

## `ChildProcess`

A running child process launched by `Process.spawn`, with streaming access to its stdin, stdout, and stderr. Closing the child, explicitly or by dropping it, kills and reaps a child that is still running.

```dream
public class ChildProcess
```

## `stdout`

The child's standard output. Keep one reader when using `read_line`, because it buffers bytes past the returned line.

```dream
public get stdout(): ProcessReader
```

## `stderr`

The child's standard error. Drain it alongside stdout so a child that fills one pipe does not stall.

```dream
public get stderr(): ProcessReader
```

## `stdin`

The child's standard input. Call `close_stdin` to signal end of input.

```dream
public get stdin(): ProcessWriter
```

## `close_stdin`

Closes the child's stdin once pending writes finish, so a child reading until end of input can complete. Returns `ECLOSED` when the child was already closed.

```dream
public fun close_stdin(
    token: Option<CancellationToken> = Option.None
): Future<Result<Unit, ProcessError>>
```

## `wait`

Waits for the child to exit and returns its exit status, negative when a signal ended it. Unread output stays readable, and the child stays open.

```dream
public fun wait(token: Option<CancellationToken> = Option.None): Future<Result<int, ProcessError>>
```

## `close`

Kills the child if it is still running and waits until it is reaped. Repeated or concurrent calls share the first outcome.

```dream
public fun close(): Future<Result<Unit, ProcessError>>
```

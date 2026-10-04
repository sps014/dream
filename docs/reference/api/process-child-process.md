# ChildProcess

**Import:** `import system.process;`

Read the [usage guide](../stdlib/process.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class ChildProcess`

Handle to a spawned child process with piped stdin/stdout/stderr (`Process.spawn`).

```dream
public class ChildProcess
```

## `write_stdin`

Writes `data` to the child's stdin. Returns `false` if the pipe is closed or the write fails.

```dream
public fun write_stdin(data: byte[]): bool
```

## `write_stdin_text`

Writes UTF-8 text to the child's stdin.

```dream
public fun write_stdin_text(text: string): bool
```

## `read_stdout`

Reads up to `max_bytes` currently buffered from stdout, blocking until at least one byte has arrived or the stream has reached end-of-file (in which case the result is empty).

```dream
public async fun read_stdout(max_bytes: int, token: Option<CancellationToken> = Option.None): byte[]
```

## `read_stdout_all`

```dream
public async fun read_stdout_all(token: Option<CancellationToken> = Option.None): byte[]
```

## `read_stdout_line`

Reads one line from stdout (without the trailing newline), or `None` at end-of-file.

```dream
public async fun read_stdout_line(token: Option<CancellationToken> = Option.None): Option<string>
```

## `read_stderr`

Reads up to `max_bytes` currently buffered from stderr.

```dream
public async fun read_stderr(max_bytes: int, token: Option<CancellationToken> = Option.None): byte[]
```

## `read_stderr_all`

```dream
public async fun read_stderr_all(token: Option<CancellationToken> = Option.None): byte[]
```

## `read_stderr_line`

Reads one line from stderr (without the trailing newline), or `None` at end-of-file.

```dream
public async fun read_stderr_line(token: Option<CancellationToken> = Option.None): Option<string>
```

## `wait`

Waits for the process to exit, returning its exit code (or a negative sentinel: `-1` if it could not be waited on, `-2` if it was terminated by a signal with no exit code).

```dream
public async fun wait(token: Option<CancellationToken> = Option.None): int
```

## `kill`

Forcibly terminates the process. Returns `false` if it had already exited or could not be killed.

```dream
public fun kill(): bool
```

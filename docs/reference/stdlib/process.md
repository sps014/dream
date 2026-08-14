# Process

**Package:** `system.process` — `import system.process;`

Runs and controls child processes: capture a command's full output with `Process.run`, or launch an interactive child with `Process.spawn` and stream its stdin/stdout/stderr. APIs are marked `@native` and `@node` — compiling for the **web** target is a **compile error** if your program references them. Native (`dream run`) and Node.js both support the full API.

```dream
import system;
import system.process;
```

## Platform notes

| Runtime | `Process.run` | `Process.spawn` |
| --- | --- | --- |
| Native (`dream run`) | `std::process::Command::output` | `std::process::Command::spawn` with piped stdio |
| Node.js | `child_process.spawnSync` | `child_process.spawn` with piped stdio |
| Browser (`--web`) | Compile error — APIs not available on target `web` | Compile error |

## `Process`

#### `Process.run(cmd: string, args: string[]): Result<ProcessOutput, ProcessError>`

Runs `cmd` with `args`, waits for it to exit, and captures stdout/stderr in full. Both are async — call from an `async fun` with `await`.

```dream
async fun main(): void {
    switch (await Process.run("git", ["status"])) {
        Ok(out) => {
            System.println(out.success());
            System.println(out.stdout);
        },
        Err(e) => System.println(e.code() + ": " + e.message()),
    }
}
```

#### `Process.run_in(cmd: string, args: string[], cwd: string): Result<ProcessOutput, ProcessError>`

Same as `run`, but launches the process with `cwd` as its working directory (an empty string keeps the caller's current working directory).

#### `Process.spawn(cmd: string, args: string[]): Result<ChildProcess, ProcessError>`

Spawns `cmd` with `args` and returns a [`ChildProcess`](#childprocess) handle immediately, without waiting for it to exit — use this for long-running or interactive processes.

```dream
async fun main(): void {
    switch (await Process.spawn("cat", [])) {
        Ok(child) => {
            child.write_stdin_text("hello\n");
            System.println((await child.read_stdout_line()).unwrap_or(""));
            child.kill();
        },
        Err(e) => System.println(e.message()),
    }
}
```

#### `Process.spawn_in(cmd: string, args: string[], cwd: string): Result<ChildProcess, ProcessError>`

Same as `spawn`, but launches the process with `cwd` as its working directory.

## `ProcessOutput`

The captured result of `Process.run`.

| Member | Type | Meaning |
| --- | --- | --- |
| `exit_code` | `int` | The process's exit code. |
| `stdout` | `string` | Everything the process wrote to stdout. |
| `stderr` | `string` | Everything the process wrote to stderr. |
| `success()` | `bool` | `true` when `exit_code == 0`. |

## `ChildProcess`

A handle to a process started with `Process.spawn`, with piped stdin/stdout/stderr.

#### `write_stdin(data: byte[]): bool` / `write_stdin_text(text: string): bool`

Writes to the child's stdin. Returns `false` if the pipe is closed or the write fails.

#### `read_stdout(max_bytes: int): byte[]` / `read_stderr(max_bytes: int): byte[]`

Reads up to `max_bytes` currently buffered from stdout/stderr. Blocks until at least one byte has arrived or the stream has reached end-of-file, in which case the result is empty. Both are async.

#### `read_stdout_line(): Option<string>` / `read_stderr_line(): Option<string>`

Reads one line (without the trailing newline), or `None` at end-of-file. Both are async.

#### `wait(): int`

Waits for the process to exit and returns its exit code (`-1` if it could not be waited on, `-2` if it was terminated by a signal with no exit code). Async.

#### `kill(): bool`

Forcibly terminates the process. Returns `false` if it had already exited or could not be killed.

## `ProcessError`

Implements [`Error`](builtins.md). `code()` is one of:

| Code | Meaning |
| --- | --- |
| `ESPAWN` | The executable could not be launched (not found, not executable, permission denied, ...). |
| `EIO` | A read/write against a running child's stdin/stdout/stderr failed. |
| `EUNSUPPORTED` | Operation not supported on the current host |

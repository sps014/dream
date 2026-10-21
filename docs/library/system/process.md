---
title: Process
summary: Launch and talk to child processes.
icon: material/application-cog-outline
api: reference/api/system-process.md
---

# Process

`system.process` launches owned child processes on native and Node. Browser launch operations return `ProcessError` with code `EUNSUPPORTED`.

```dream
import system;
import system.process;

async fun main(): void {
    let options = ProcessOptions();
    options.env["APP_MODE"] = "test";
    options.timeout_ms = 5000;
    let output = Process.run("example-tool", ["--check"], options: options).await;
    switch (output) {
        Ok(value) => System.println(value.stdout),
        Err(error) => System.println(error.message()),
    }
}
```

The simple `run(command, arguments)` overload uses default options. `run` feeds `options.input` to stdin, closes it, drains stdout and stderr concurrently, and waits for exit. Argument boundaries preserve empty strings, Unicode, whitespace, and newlines through length-prefixed UTF-8 encoding. Commands launch directly without shell interpolation.

Options inherit the environment and working directory by default. Set `env` overrides, `clear_env`, `cwd`, and `timeout_ms` explicitly. Captured output defaults to an 8 MiB limit per stream; exceeding `max_output_bytes` returns `ELIMIT`. Input is buffered in the options snapshot. A child that rejects input can cause an I/O error.

A nonzero exit status is a successful transport result; `run_checked` turns it into `EEXIT`. `which` searches for an existing nondirectory path, without validating execute permissions.

## Process functions

Every launching function accepts an optional trailing `token: Option<CancellationToken>`.

| Function | Returns | Effect |
| --- | --- | --- |
| `Process.run(cmd, args)` | `Future<Result<ProcessOutput, ProcessError>>` | Runs with default options and captures both outputs. |
| `Process.run(cmd, args, options)` | `Future<Result<ProcessOutput, ProcessError>>` | Runs with the given `ProcessOptions`. |
| `Process.run_in(cmd, args, cwd)` | `Future<Result<ProcessOutput, ProcessError>>` | Runs in the directory `cwd`. |
| `Process.run_checked(cmd, args)` | `Result<ProcessOutput, ProcessError>` (async) | Like `run`, but a nonzero exit returns `EEXIT`. |
| `Process.spawn(cmd, args)` | `Future<Result<ChildProcess, ProcessError>>` | Starts a child with streaming pipes. |
| `Process.spawn(cmd, args, options)` | `Future<Result<ChildProcess, ProcessError>>` | Starts a child with the given `ProcessOptions`. |
| `Process.spawn_in(cmd, args, cwd)` | `Future<Result<ChildProcess, ProcessError>>` | Starts a child in the directory `cwd`. |
| `Process.which(cmd)` | `Option<string>` | Finds a command path directly or on `PATH`; native and Node only. |

## Options

| Field | Default | Effect |
| --- | --- | --- |
| `cwd` | `""` | Working directory; empty inherits the parent's directory. |
| `env` | empty `Map<string, string>` | Variables added on top of the inherited environment. |
| `clear_env` | `false` | When true, the child starts with only the `env` entries. |
| `timeout_ms` | `0` | Wall-clock deadline in milliseconds; zero means no deadline. |
| `max_output_bytes` | `8388608` (8 MiB) | Limit per captured stream for `run`; exceeding it returns `ELIMIT`. |
| `input` | empty `byte[]` | Bytes `run` writes to stdin before closing it. |

## Captured output

| Property | Type | Meaning |
| --- | --- | --- |
| `exit_code` | `int` | Exit status; negative values mean the process was ended by a signal. |
| `stdout` | `string` | Captured standard output, decoded as UTF-8. |
| `stderr` | `string` | Captured standard error, decoded as UTF-8. |
| `success` | `bool` | True when the exit status is zero. |

## Streaming pipes

```dream
let child = Process.spawn("example-tool", ["--stream"]).await?;
let stdout = child.stdout;
let stderr = child.stderr;
let stdin = child.stdin;
let written = stdin.write_async(Encoding.utf8_encode("input\n")).await?;
child.close_stdin().await?;
let line = stdout.read_line().await?;
let status = child.wait().await?;
child.close().await?;
```

| Member | Returns | Effect |
| --- | --- | --- |
| `child.stdout` / `child.stderr` | `ProcessReader` | Borrows an output pipe. |
| `child.stdin` | `ProcessWriter` | Borrows the input pipe. |
| `child.close_stdin()` | `Future<Result<Unit, ProcessError>>` | Closes stdin after pending writes, sending EOF to the child. |
| `child.wait()` | `Future<Result<int, ProcessError>>` | Waits for exit and returns the exit status. |
| `child.close()` | `Future<Result<Unit, ProcessError>>` | Terminates if needed, reaps the child, and releases it. |
| `reader.read_async(max_bytes)` | `Result<byte[], IoError>` (async) | Reads up to `max_bytes`; an empty result means EOF. |
| `reader.read_line(max_bytes = 1048576)` | `Future<Result<Option<string>, IoError>>` | Reads one line without its LF or CRLF; `None` at EOF. |
| `writer.write_async(data)` | `Result<int, IoError>` (async) | Writes some or all bytes and returns the count. |
| `writer.flush_async()` | `Result<Unit, IoError>` (async) | Checks the pipe is open; there is no buffer to flush. |

`stdin` implements `AsyncWriter`; `stdout` and `stderr` implement `AsyncReader`. Reads and writes return partial progress and propagate failures. `Streams.copy_async` connects process pipes to files, HTTP bodies, and other async streams. Writes are not guaranteed to consume the entire buffer; retry the remaining bytes or use the copy helper.

Drain stdout and stderr concurrently when a command can fill both pipes. Waiting for process exit before draining can deadlock on output backpressure. A retained `ProcessReader` provides buffered `read_line`, with LF/CRLF handling, a line-size limit, and final unterminated lines.

`wait` leaves the handle and unread pipes available. `close().await` terminates unfinished work, reaps the direct child, and closes ownership idempotently. Concurrent closes share the outcome. A destructor requests nonthrowing cleanup; explicit close reports errors. Pipe wrappers retain their owning child.

Cancellable operations inherit the current structured task's token when omitted. Cancelling `run` or its deadline terminates and reaps the owned child before returning `ECANCELLED` or `ETIMEOUT`. Cancelling an individual pipe operation preserves completed side effects and leaves the child available for explicit closure. Spawned-child deadlines also terminate and reap the child. Descendant process trees are not managed as a group.

## Error codes

`ProcessError.code()` and the `IoError` codes returned by pipe operations include:

| Code | Meaning |
| --- | --- |
| `ESPAWN` | The command could not be started, for example because it was not found or is not executable. |
| `EEXIT` | `run_checked` saw a nonzero exit status. |
| `ETIMEOUT` | The `timeout_ms` deadline passed; the child was terminated. |
| `ECANCELLED` | The operation's cancellation token was cancelled. |
| `ELIMIT` | Captured output exceeded `max_output_bytes`, or a line exceeded the `read_line` limit. |
| `EINVAL` | A request was malformed, such as an environment name containing `=`. |
| `ECLOSED` | The child or the pipe was already closed. |
| `EIO` | Reading from or writing to the child failed. |
| `EUNSUPPORTED` | The host cannot launch processes, as in browsers. |

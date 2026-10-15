# Process

`system.process` launches owned child processes on native and Node. Browser launch operations return `ProcessError` with code `EUNSUPPORTED`.

```dream
import system;
import system.process;

async fun main(): void {
    let options = ProcessOptions();
    options.env.set("APP_MODE", "test");
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

`ProcessOutput` exposes `exit_code`, `stdout`, `stderr`, and `success`. A nonzero exit status is a successful transport result; `run_checked` turns it into `EEXIT`. `run_in` and `spawn_in` provide working-directory conveniences. `which` searches for an existing nondirectory path, without validating execute permissions.

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

`stdin` implements `AsyncWriter`; `stdout` and `stderr` implement `AsyncReader`. Reads and writes return partial progress and propagate failures. `Streams.copy_async` connects process pipes to files, HTTP bodies, and other async streams. Writes are not guaranteed to consume the entire buffer; retry the remaining bytes or use the copy helper.

Drain stdout and stderr concurrently when a command can fill both pipes. Waiting for process exit before draining can deadlock on output backpressure. A retained `ProcessReader` provides buffered `read_line`, with LF/CRLF handling, a line-size limit, and final unterminated lines.

`wait` leaves the handle and unread pipes available. `close().await` terminates unfinished work, reaps the direct child, and closes ownership idempotently. Concurrent closes share the outcome. A destructor requests nonthrowing cleanup; explicit close reports errors. Pipe wrappers retain their owning child.

Cancellable operations inherit the current structured task's token when omitted. Cancelling `run` or its deadline terminates and reaps the owned child before returning `ECANCELLED` or `ETIMEOUT`. Cancelling an individual pipe operation preserves completed side effects and leaves the child available for explicit closure. Spawned-child deadlines also terminate and reap the child. Descendant process trees are not managed as a group.

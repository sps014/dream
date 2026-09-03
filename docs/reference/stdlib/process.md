# Process

**Import:** `import system.process;`

Run other programs. **Native and Node only** — a web build that mentions these APIs is a compile error.

```dream
import system;
import system.process;

async fun main(): void {
    switch (await Process.run("git", ["status"])) {
        Ok(out) => System.println(out.stdout),
        Err(e) => System.println(e.message()),
    }
}
```

| Call | Meaning |
| --- | --- |
| `await Process.run(cmd, args)` | run to completion, capture output |
| `await Process.run_checked(...)` | same, `Err` when the exit code is not 0 |
| `await Process.run_in(cmd, args, cwd)` | same, with a working directory |
| `Process.which(cmd)` | first matching executable on `PATH` |
| `await Process.spawn(cmd, args)` | start a child, keep a handle |
| `await Process.spawn_in(...)` | spawn with `cwd` |

`ProcessOutput`: `.success()`, `.stdout`, `.stderr`, exit code.

`ChildProcess`: `write_stdin` / `write_stdin_text`, `read_stdout` / `read_stderr` (bytes, a line, or `*_all`), `wait()`, `kill()`.

Failures are `ProcessError` (`message()` / `code()`, including `ECANCELLED` when a `token` argument is already cancelled). Async methods take an optional last `CancellationToken`.

# Process

**Import:** `import system.process;`

Read the [process guide](../../library/system/process.md) for launch settings, cancellation, and streaming ownership.

## `Process`

Launches child processes and finds commands on `PATH`. Available in native programs and in Node.js; browsers have no process API.

```dream
public static class Process
```

## `run`

Runs a command to completion with default options: writes nothing to stdin, captures stdout and stderr as UTF-8 text, and returns them with the exit status. A nonzero exit status is not an error; check `ProcessOutput.success`. Fails with `ESPAWN` when the command cannot be launched.

```dream
public static fun run(
    cmd: string,
    args: string[],
    token: Option<CancellationToken> = Option.None
): Future<Result<ProcessOutput, ProcessError>>
```

## `run`

Runs a command with explicit options: working directory, environment, stdin bytes, output limit, and deadline. Fails with `ETIMEOUT` after the deadline, `ELIMIT` when an output exceeds the limit, and `ECANCELLED` when `token` is cancelled; each kills the child first.

```dream
public static fun run(
    cmd: string,
    args: string[],
    options: ProcessOptions,
    token: Option<CancellationToken> = Option.None
): Future<Result<ProcessOutput, ProcessError>>
```

## `run_in`

Runs a command in `cwd` with otherwise default options.

```dream
public static fun run_in(
    cmd: string,
    args: string[],
    cwd: string,
    token: Option<CancellationToken> = Option.None
): Future<Result<ProcessOutput, ProcessError>>
```

## `spawn`

Launches a command and returns the running child with streaming stdin, stdout, and stderr pipes. Fails with `ESPAWN` when the command cannot be launched.

```dream
public static fun spawn(
    cmd: string,
    args: string[],
    token: Option<CancellationToken> = Option.None
): Future<Result<ChildProcess, ProcessError>>
```

## `spawn`

Launches a command with an explicit working directory and environment. The deadline, output limit, and input options apply only to `run`.

```dream
public static fun spawn(
    cmd: string,
    args: string[],
    options: ProcessOptions,
    token: Option<CancellationToken> = Option.None
): Future<Result<ChildProcess, ProcessError>>
```

## `spawn_in`

Launches a child in `cwd` with otherwise default options.

```dream
public static fun spawn_in(
    cmd: string,
    args: string[],
    cwd: string,
    token: Option<CancellationToken> = Option.None
): Future<Result<ChildProcess, ProcessError>>
```

## `run_checked`

Runs a command like `run`, and also fails with `EEXIT` when it exits with a nonzero status.

```dream
public static async fun run_checked(
    cmd: string,
    args: string[],
    token: Option<CancellationToken> = Option.None
): Result<ProcessOutput, ProcessError>
```

## `which`

Finds an existing nondirectory command path, directly or by searching `PATH`. This does not validate execute permissions or expand platform-specific extensions.

```dream
public static fun which(cmd: string): Option<string>
```

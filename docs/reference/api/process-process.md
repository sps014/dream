# Process

**Import:** `import system.process;`

Read the [usage guide](../stdlib/process.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `static class Process`

Runs and controls child processes (native only — the browser host reports `ProcessError.unsupported` for every operation; Node supports both `run` and `spawn`).

```dream
public static class Process
```

## `run`

Runs `cmd` with `args` in the current working directory, waits for it to exit, and captures stdout/stderr in full.

```dream
public static async fun run(cmd: string, args: string[], token: Option<CancellationToken> = Option.None): Result<ProcessOutput, ProcessError>
```

## `run_in`

Same as `run`, but launches the process with `cwd` as its working directory (an empty string keeps the caller's current working directory).

```dream
public static async fun run_in(cmd: string, args: string[], cwd: string, token: Option<CancellationToken> = Option.None): Result<ProcessOutput, ProcessError>
```

## `spawn`

Spawns `cmd` with `args` in the current working directory, returning a handle for interactive stdin/stdout/stderr access and lifecycle control.

```dream
public static async fun spawn(cmd: string, args: string[], token: Option<CancellationToken> = Option.None): Result<ChildProcess, ProcessError>
```

## `spawn_in`

Same as `spawn`, but launches the process with `cwd` as its working directory.

```dream
public static async fun spawn_in(cmd: string, args: string[], cwd: string, token: Option<CancellationToken> = Option.None): Result<ChildProcess, ProcessError>
```

## `run_checked`

```dream
public static async fun run_checked(cmd: string, args: string[], token: Option<CancellationToken> = Option.None): Result<ProcessOutput, ProcessError>
```

## `which`

```dream
public static fun which(cmd: string): Option<string>
```

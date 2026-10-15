# Process

**Import:** `import system.process;`

Read the [process guide](../stdlib/process.md) for launch settings, cancellation, and streaming ownership.

## `Process`

Launches processes asynchronously on native and Node; browser operations return EUNSUPPORTED.

```dream
public static class Process
```

## `run`

Runs a command, concurrently drains both outputs, and closes stdin after sending input.

```dream
public static fun run(cmd: string, args: string[], token: Option<CancellationToken> = Option.None): Future<Result<ProcessOutput, ProcessError>>
```

## `run`

Runs with explicit environment, input, output limits, and an optional deadline.

```dream
public static fun run(cmd: string, args: string[], options: ProcessOptions, token: Option<CancellationToken> = Option.None): Future<Result<ProcessOutput, ProcessError>>
```

## `run_in`

Runs a command in a specific working directory with otherwise default options.

```dream
public static fun run_in(cmd: string, args: string[], cwd: string, token: Option<CancellationToken> = Option.None): Future<Result<ProcessOutput, ProcessError>>
```

## `spawn`

Spawns an owned child exposing streaming stdin, stdout, and stderr pipes.

```dream
public static fun spawn(cmd: string, args: string[], token: Option<CancellationToken> = Option.None): Future<Result<ChildProcess, ProcessError>>
```

## `spawn`

Spawns with explicit launch settings; child closure terminates and reaps unfinished work.

```dream
public static fun spawn(cmd: string, args: string[], options: ProcessOptions, token: Option<CancellationToken> = Option.None): Future<Result<ChildProcess, ProcessError>>
```

## `spawn_in`

Spawns a child in a specific directory with otherwise default options.

```dream
public static fun spawn_in(cmd: string, args: string[], cwd: string, token: Option<CancellationToken> = Option.None): Future<Result<ChildProcess, ProcessError>>
```

## `run_checked`

Captures output and additionally treats nonzero exit statuses as EEXIT errors.

```dream
public static async fun run_checked(cmd: string, args: string[], token: Option<CancellationToken> = Option.None): Result<ProcessOutput, ProcessError>
```

## `which`

Finds an existing nondirectory command path, directly or by searching `PATH`. This does not validate execute permissions or expand platform-specific extensions.

```dream
public static fun which(cmd: string): Option<string>
```

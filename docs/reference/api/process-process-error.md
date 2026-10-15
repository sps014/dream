# Process Error

**Import:** `import system.process;`

Read the [process guide](../stdlib/process.md) for launch settings, cancellation, and streaming ownership.

## `ProcessError`

Failure launching or communicating with a child process, implementing `Error`.

```dream
public class ProcessError : Error
```

## `constructor`

Creates an error with a machine code and message.

```dream
public constructor(code: string, message: string)
```

## `message`

Human-readable description.

```dream
public fun message(): string
```

## `code`

Stable machine code (`ESPAWN`, `EIO`, `EUNSUPPORTED`, …).

```dream
public fun code(): string
```

## `spawn_failed`

The executable could not be launched (not found, not executable, permission denied, ...).

```dream
public static fun spawn_failed(message: string): ProcessError
```

## `io`

A read/write against a running child's stdin/stdout/stderr failed.

```dream
public static fun io(message: string): ProcessError
```

## `unsupported`

Process control is not available on the current host (e.g. the browser).

```dream
public static fun unsupported(message: string): ProcessError
```

## `failed`

Reports a nonzero exit status from run_checked.

```dream
public static fun failed(message: string): ProcessError
```

## `cancelled`

Cooperative cancellation (`CancellationToken`).

```dream
public static fun cancelled(): ProcessError
```

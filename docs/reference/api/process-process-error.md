# ProcessError

**Import:** `import system.process;`

Read the [usage guide](../stdlib/process.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class ProcessError : Error`

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

```dream
public static fun failed(message: string): ProcessError
```

## `cancelled`

Cooperative cancellation (`CancellationToken`).

```dream
public static fun cancelled(): ProcessError
```

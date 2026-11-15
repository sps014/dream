# Process Options

**Import:** `import system.process;`

Read the [process guide](../../library/system/process.md) for launch settings, cancellation, and streaming ownership.

## `ProcessOptions`

Process launch settings, snapshotted before the launch operation is submitted.

```dream
public class ProcessOptions
```

## `cwd`

Working directory; empty inherits the parent directory.

```dream
public cwd: string
```

## `env`

Environment overrides applied after the inherited environment.

```dream
public env: Map<string, string>
```

## `clear_env`

Starts with an empty environment when true.

```dream
public clear_env: bool
```

## `timeout_ms`

Wall-clock deadline in milliseconds; zero disables the deadline.

```dream
public timeout_ms: int
```

## `max_output_bytes`

Maximum captured bytes per output stream for run; defaults to 8 MiB.

```dream
public max_output_bytes: int
```

## `input`

Bytes written to stdin by run before closing the input pipe.

```dream
public input: byte[]
```

## `constructor`

Creates options that inherit the environment and directory with empty stdin.

```dream
public constructor()
```

# Process Options

**Import:** `import system.process;`

Read the [process guide](../stdlib/process.md) for launch settings, cancellation, and streaming ownership.

## `ProcessOptions`

Process launch settings, snapshotted before the launch operation is submitted.

```dream
public class ProcessOptions
```

## `public cwd: string`

Working directory; empty inherits the parent directory.

```dream
public cwd: string
```

## `public env: Map<string, string>`

Environment overrides applied after the inherited environment.

```dream
public env: Map<string, string>
```

## `public clear_env: bool`

Starts with an empty environment when true.

```dream
public clear_env: bool
```

## `public timeout_ms: int`

Wall-clock deadline in milliseconds; zero disables the deadline.

```dream
public timeout_ms: int
```

## `public max_output_bytes: int`

Maximum captured bytes per output stream for run; defaults to 8 MiB.

```dream
public max_output_bytes: int
```

## `public input: byte[]`

Bytes written to stdin by run before closing the input pipe.

```dream
public input: byte[]
```

## `constructor`

Creates options that inherit the environment and directory with empty stdin.

```dream
public constructor()
```

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

Wall-clock deadline for `Process.run` in milliseconds; zero disables it. When it passes, the child is killed and `run` fails with `ETIMEOUT`. Spawned children have no deadline.

```dream
public timeout_ms: int
```

## `max_output_bytes`

Maximum captured bytes per output stream for `Process.run`; defaults to 8 MiB, and zero also selects the default. A larger output kills the child and fails with `ELIMIT`.

```dream
public max_output_bytes: int
```

## `input`

Bytes `Process.run` writes to stdin before closing it. A child that exits without reading all of them does not cause an error.

```dream
public input: byte[]
```

## `constructor`

Creates options that inherit the environment and directory with empty stdin.

```dream
public constructor()
```

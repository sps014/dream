# Process Output

**Import:** `import system.process;`

Read the [process guide](../../library/system/process.md) for launch settings, cancellation, and streaming ownership.

## `ProcessOutput`

The captured result of `Process.run`: exit code plus fully buffered stdout/stderr text.

```dream
public class ProcessOutput
```

## `exit_code`

Process exit status; negative values represent signal termination.

```dream
public get exit_code(): int
```

## `stdout`

Captured standard output decoded as UTF-8 text.

```dream
public get stdout(): string
```

## `stderr`

Captured standard error decoded as UTF-8 text.

```dream
public get stderr(): string
```

## `success`

True when the process exited with status zero.

```dream
public get success(): bool
```

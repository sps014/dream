# ProcessOutput

**Import:** `import system.process;`

Read the [usage guide](../stdlib/process.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class ProcessOutput`

The captured result of `Process.run`: exit code plus fully buffered stdout/stderr text.

```dream
public class ProcessOutput
```

## `exit_code`

```dream
public get exit_code(): int
```

## `stdout`

```dream
public get stdout(): string
```

## `stderr`

```dream
public get stderr(): string
```

## `success`

```dream
public get success(): bool
```

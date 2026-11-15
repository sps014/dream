# LogLevel

**Import:** `import system.logging;`

Read the [usage guide](../../library/system/logging.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `LogLevel`

Severity levels ordered from most to least verbose.

```dream
public enum LogLevel
```

## `Trace`

Very detailed diagnostic messages.

```dream
Trace
```

## `Debug`

Diagnostic messages useful during development.

```dream
Debug
```

## `Info`

Messages describing normal operation.

```dream
Info
```

## `Warn`

Warnings about recoverable problems.

```dream
Warn
```

## `Error`

Messages reporting failures.

```dream
Error
```

## `level_name`

Uppercase label for a severity level (`TRACE` … `ERROR`).

```dream
public fun level_name(level: LogLevel): string
```

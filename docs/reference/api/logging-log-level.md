# LogLevel

**Import:** `import system.logging;`

Read the [usage guide](../stdlib/logging.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `enum LogLevel`

Severity levels ordered from most to least verbose.

```dream
public enum LogLevel
```

## `Trace`

```dream
Trace
```

## `Debug`

```dream
Debug
```

## `Info`

```dream
Info
```

## `Warn`

```dream
Warn
```

## `Error`

```dream
Error
```

## `level_name`

Uppercase label for a severity level (`TRACE` … `ERROR`).

```dream
public fun level_name(level: LogLevel): string
```

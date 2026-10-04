# LogRecord

**Import:** `import system.logging;`

Read the [usage guide](../stdlib/logging.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class LogRecord`

One log event ready for handlers.

```dream
public class LogRecord
```

## `level: LogLevel`

Severity of this record.

```dream
public level: LogLevel
```

## `name: string`

Logger name that produced this record.

```dream
public name: string
```

## `message: string`

Message text.

```dream
public message: string
```

## `timestamp_ms: long`

UTC epoch milliseconds when the record was created.

```dream
public timestamp_ms: long
```

# LogHandler

**Import:** `import system.logging;`

Read the [usage guide](../../library/system/logging.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `LogHandler`

Sink that receives log records.

```dream
public interface LogHandler
```

## `emit`

Processes one log record.

```dream
fun emit(record: LogRecord): void
```

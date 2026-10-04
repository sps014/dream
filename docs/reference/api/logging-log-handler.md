# LogHandler

**Import:** `import system.logging;`

Read the [usage guide](../stdlib/logging.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `interface LogHandler`

Sink that receives log records.

```dream
public interface LogHandler
```

## `emit`

```dream
fun emit(record: LogRecord): void
```

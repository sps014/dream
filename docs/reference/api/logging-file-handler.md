# FileHandler

**Import:** `import system.logging;`

Read the [usage guide](../stdlib/logging.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class FileHandler : LogHandler`

Appends formatted lines to a file (synchronous host write).

```dream
public class FileHandler : LogHandler
```

## `constructor`

Creates a handler that appends to `path`.

```dream
public constructor(path: string)
```

## `emit`

Appends one formatted line to the file.

```dream
public fun emit(record: LogRecord): void
```

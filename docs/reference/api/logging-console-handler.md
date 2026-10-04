# ConsoleHandler

**Import:** `import system.logging;`

Read the [usage guide](../stdlib/logging.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class ConsoleHandler : LogHandler`

Writes formatted lines to stdout via `System.println`.

```dream
public class ConsoleHandler : LogHandler
```

## `constructor`

Creates a console handler.

```dream
public constructor()
```

## `emit`

Prints `[LEVEL] name: message`.

```dream
public fun emit(record: LogRecord): void
```

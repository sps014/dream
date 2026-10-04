# Logging


Logging records events while your app runs. Give each logger a name, choose a minimum severity, and add handlers to send records to the console, a file, or your own destination.

**Import:** `import system.logging;`

Named loggers, levels, and handlers (console or file).

```dream
import system;
import system.logging;

fun main() {
    let log = Logger.get("app");
    log.add_handler(ConsoleHandler());
    log.level = LogLevel.Debug;
    log.info("ready");
}
```

Levels, low to high: `Trace`, `Debug`, `Info`, `Warn`, `Error`. Records below the logger’s level are dropped. `level_name(LogLevel.Info)` is `"Info"`.

| Call | Meaning |
| --- | --- |
| `Logger.get(name)` | shared logger by name |
| `Logger(name)` | a new logger |
| `log.level` | minimum level |
| `add_handler(handler)` | where records go |
| `trace` / `debug` / `info` / `warn` / `error(msg)` | emit |

Handlers: `ConsoleHandler()`, `FileHandler(path)`, or your own `LogHandler.emit(record)`.

## Write your own handler

A `LogHandler` implements `emit(record: LogRecord): void`. Each record has four public fields:

| Field | Type | Meaning |
| --- | --- | --- |
| `level` | `LogLevel` | Severity of this event |
| `name` | `string` | Name of the logger that produced it |
| `message` | `string` | Event text |
| `timestamp_ms` | `long` | UTC milliseconds since the Unix epoch |

```dream
import system;
import system.logging;

class ShortHandler : LogHandler {
    public fun emit(record: LogRecord): void {
        System.println(record.name + ": " + record.message);
    }
}

fun main() {
    let logger = Logger("app");
    logger.add_handler(ShortHandler());
    logger.info("ready");
}
```

This prints `app: ready`. Choose `Logger(name)` for a new logger or `Logger.get(name)` to reuse a named logger. Avoid logging credentials or private data in either kind of handler.

See [LogRecord](../api/logging-log-record.md), [LogHandler](../api/logging-log-handler.md), and [Logger](../api/logging-logger.md) for all declarations.

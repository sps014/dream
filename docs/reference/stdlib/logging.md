# Logging

**Import:** `import system.logging;`

Named loggers, levels, and handlers (console or file).

```dream
import system;
import system.logging;

fun main() {
    let log = Logger.get("app");
    log.add_handler(ConsoleHandler());
    log.set_level(LogLevel.Debug);
    log.info("ready");
}
```

Levels, low to high: `Trace`, `Debug`, `Info`, `Warn`, `Error`. Records below the logger’s level are dropped. `level_name(LogLevel.Info)` is `"Info"`.

| Call | Meaning |
| --- | --- |
| `Logger.get(name)` | shared logger by name |
| `Logger(name)` | a new logger |
| `set_level(level)` | minimum level |
| `add_handler(handler)` | where records go |
| `trace` / `debug` / `info` / `warn` / `error(msg)` | emit |

Handlers: `ConsoleHandler()`, `FileHandler(path)`, or your own `LogHandler.emit(record)`.

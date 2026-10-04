# Logger

**Import:** `import system.logging;`

Read the [usage guide](../stdlib/logging.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class Logger`

Named logger with a minimum level and zero or more handlers.

```dream
public class Logger
```

## `constructor`

```dream
public constructor(name: string)
```

## `get`

Returns the named logger, creating it on first use.

```dream
public static fun get(name: string): Logger
```

## `level`

```dream
public get level(): LogLevel
```

## `level`

```dream
public set level(level: LogLevel)
```

## `add_handler`

Registers an additional handler.

```dream
public fun add_handler(h: LogHandler): void
```

## `trace`

Logs at Trace.

```dream
public fun trace(msg: string): void
```

## `debug`

Logs at Debug.

```dream
public fun debug(msg: string): void
```

## `info`

Logs at Info.

```dream
public fun info(msg: string): void
```

## `warn`

Logs at Warn.

```dream
public fun warn(msg: string): void
```

## `error`

Logs at Error.

```dream
public fun error(msg: string): void
```

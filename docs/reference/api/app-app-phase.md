# App Phase

**Import:** `import system.app;`

Read the [usage guide](../../library/system/app.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `AppPhase`

Where the program is in its lifetime, as reported by `App.phase`.  Normal code does not need the phase: use the `App.on_*` hooks and `App.stopping` instead. It exists for diagnostics and tests.

```dream
public enum AppPhase
```

## `Starting`

`App.run` is running the `start` function.

```dream
Starting
```

## `Running`

`start` finished successfully and no shutdown has begun.

```dream
Running
```

## `Asleep`

The computer is asleep: `on_sleep` ran and `on_wake` has not run yet.

```dream
Asleep
```

## `Stopping`

Shutdown began. `App.stopping` is cancelled and `on_stop` cleanups run.

```dream
Stopping
```

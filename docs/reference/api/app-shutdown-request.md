# Shutdown Request

**Import:** `import system.app;`

Read the [usage guide](../../library/system/app.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `ShutdownRequest`

A request to stop the program, passed to `Application.on_shutdown_requested` hooks.  Every hook sees the same request. When any hook calls `veto` on a request that allows it, the program keeps running and `Application.stopping` stays uncancelled.

```dream
public class ShutdownRequest
```

## `reason`

Why the program is being asked to stop.

```dream
public get reason(): ShutdownReason
```

## `can_veto`

True when `veto` can keep the program running. Signals, `Application.quit`, and the final notice that the session is ending cannot be vetoed.

```dream
public get can_veto(): bool
```

## `veto`

Asks to keep the program running, for example while a document has unsaved changes. Does nothing when `can_veto` is false.

```dream
public fun veto(): void
```

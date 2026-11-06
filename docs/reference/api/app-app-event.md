# App Event

**Import:** `import system.app;`

Read the [usage guide](../../library/system/app.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `AppEvent`

An operating-system event to inject with `Application.simulate` in tests.  Each variant reaches the same hooks, properties, and transition checks as the real event. Programs never receive `AppEvent` values; they register `Application.on_*` hooks.

```dream
public enum AppEvent
```

## `ShutdownRequested`

Something asked the program to stop. `SessionEnding` is simulated as the first, vetoable request.

```dream
ShutdownRequested(reason: ShutdownReason)
```

## `Sleeping`

The computer is about to sleep. Runs `on_sleep` hooks.

```dream
Sleeping
```

## `Woke`

The computer woke up after `slept_ms` milliseconds, or `-1` when unknown. Runs `on_wake` hooks.

```dream
Woke(slept_ms: int)
```

## `SessionLock`

The user session was locked (`true`) or unlocked (`false`). Runs `on_session_lock` hooks.

```dream
SessionLock(locked: bool)
```

## `DisplayPower`

The displays turned on (`true`) or went to sleep (`false`). Runs `on_display_power` hooks.

```dream
DisplayPower(on: bool)
```

## `Active`

The app came to the foreground (`true`) or went to the background (`false`). Runs `on_active` hooks.

```dream
Active(active: bool)
```

## `PowerSourceChanged`

The computer switched power source. Runs `on_power_source` hooks.

```dream
PowerSourceChanged(source: PowerSource)
```

## `ThemeChanged`

The user changed the system color scheme. Runs `on_theme` hooks.

```dream
ThemeChanged(theme: SystemTheme)
```

## `LocaleChanged`

The user changed the system language or regional formats. Runs `on_locale` hooks.

```dream
LocaleChanged
```

## `LowMemory`

The operating system reported that memory is running low. Runs `on_low_memory` hooks.

```dream
LowMemory
```

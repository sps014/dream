# Time

**Import:** `import system;`

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `static class Time`

Timing utilities.

```dream
public static class Time
```

## `sleep_host`

Schedules a cooperative timer. Prefer `Time.sleep` for cancellation-token handling.

```dream
public static extern async fun sleep_host(ms: int): void
```

## `sleep`

Cooperative / virtual-clock sleep (scheduler timer queue). Prefer `Time.delay` for wall-clock animation pacing in the browser.

```dream
public static async fun sleep(ms: int, token: Option<CancellationToken> = Option.None): void
```

## `delay`

Wall-clock delay (`setTimeout` in the browser; real sleep natively).

```dream
public static async fun delay(ms: int, token: Option<CancellationToken> = Option.None): void
```

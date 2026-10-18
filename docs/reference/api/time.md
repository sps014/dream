# Time

**Import:** `import system;`

## `Time`

Timing utilities.

```dream
public static class Time
```

## `sleep`

Suspends on the scheduler timer and returns cancellation instead of panicking.

```dream
public static fun sleep(
    ms: int,
    token: Option<CancellationToken> = Option.None
): Future<Result<bool, CancelledError>>
```

## `delay`

Suspends until the wall-clock timer expires or inherited cancellation is requested.

```dream
public static fun delay(
    ms: int,
    token: Option<CancellationToken> = Option.None
): Future<Result<bool, CancelledError>>
```

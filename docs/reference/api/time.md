# Time

**Import:** `import system;`

## `Time`

Timing utilities.

```dream
public static class Time
```

## `sleep`

Suspends the calling task for at least `ms` milliseconds on Dream's scheduler timer. In a browser or Node module the timer runs on a virtual clock that jumps forward when every task is waiting, so it never yields to the JavaScript event loop; use `delay` to let host callbacks run. Returns `Err(CancelledError)` when `token` is cancelled first.

```dream
public static fun sleep(
    ms: int,
    token: Option<CancellationToken> = Option.None
): Future<Result<Unit, CancelledError>>
```

## `delay`

Suspends the calling task for at least `ms` milliseconds of real time. In a browser or Node module this waits on a JavaScript timer, so the host event loop (I/O callbacks, rendering) runs meanwhile; natively it behaves like `sleep`. Returns `Err(CancelledError)` when `token` is cancelled first.

```dream
public static fun delay(
    ms: int,
    token: Option<CancellationToken> = Option.None
): Future<Result<Unit, CancelledError>>
```

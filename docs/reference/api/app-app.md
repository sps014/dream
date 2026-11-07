# App

**Import:** `import system.app;`

## `App`

The lifetime of the whole program: startup, shutdown, and operating-system events such as sleep, session lock, and theme changes.  Wrap the program in `App.run`. Ctrl+C and `SIGTERM` then stop it cleanly: the `stopping` token is cancelled, `on_stop` cleanups run newest first, and `run` returns the exit code for `main` to return. Register hooks for the events you care about; each hook returns a `Subscription` that can remove it.  Hooks run on the program's main scheduler, one event at a time, in registration order. They are not awaited, so they must return promptly; start a task for longer work.

```dream
public static class App
```

## `run`

Runs the program. `start` performs startup and may register hooks and cleanups.  After `start` returns `Ok`, the program keeps running until it is asked to stop: by `quit`, Ctrl+C, `SIGTERM`, or an operating-system shutdown that no hook vetoes. `start` may also keep running for the whole program, for example while serving requests, as long as it returns once `stopping` is cancelled. When `start` returns `Err`, the error is printed to standard error, cleanups registered so far run, and the exit code is `1`.  Shutdown cancels `stopping`, waits for `start` to return if it is still running, then runs `on_stop` cleanups in reverse registration order. All of this shares `shutdown_timeout_ms` milliseconds, counted from the start of shutdown: a `start` or cleanup still running when the time is up is cancelled and reported, and the remaining cleanups still run.  Returns the exit code: the value given to `quit`, `130` after Ctrl+C, `143` after `SIGTERM`, `1` after a failed `start`, and `0` otherwise. Calling `run` while another `run` is active prints an error and returns `1`.

```dream
public static async fun run(
    start: fun(): Future<Result<Unit, Error>>,
    shutdown_timeout_ms: int = 5000
): int
```

## `on_stop`

Registers `cleanup` to run during shutdown. Cleanups run newest first, so register each one right after acquiring the resource it releases. A cleanup registered during startup also runs when startup fails or is cancelled.  Returns `Err(AppError)` with code `ESTOPPING` once shutdown has begun, because the cleanup would never run.

```dream
public static fun on_stop(cleanup: fun(): Future<void>): Result<Subscription, AppError>
```

## `on_shutdown_requested`

Runs `hook` when something asks the program to stop: Ctrl+C, `SIGTERM`, `quit`, an operating-system quit, or the user session ending. Call `ShutdownRequest.veto` to keep running when the request allows it. Only the first accepted request is delivered; later requests are ignored once shutdown begins.

```dream
public static fun on_shutdown_requested(hook: fun(ShutdownRequest): void): Subscription
```

## `on_sleep`

Runs `hook` just before the computer sleeps. Save anything that must survive a power loss, quickly: the operating system waits at most about half a second.

```dream
public static fun on_sleep(hook: fun(): void): Subscription
```

## `on_wake`

Runs `hook` after the computer wakes, with how long it slept in milliseconds, or `-1` when the platform cannot tell. Measure gaps with this value: wall-clock time can jump across sleep.

```dream
public static fun on_wake(hook: fun(int): void): Subscription
```

## `on_session_lock`

Runs `hook` with `true` when the user session is locked, or another user switches in, and with `false` when it is unlocked again. Use it to hide sensitive content. Never fires in browsers, Node, or on Linux.

```dream
public static fun on_session_lock(hook: fun(bool): void): Subscription
```

## `on_display_power`

Runs `hook` with `false` when the displays go to sleep and `true` when they turn back on. The computer and the program keep running meanwhile. Never fires in browsers, Node, or on Linux.

```dream
public static fun on_display_power(hook: fun(bool): void): Subscription
```

## `on_active`

Runs `hook` with `true` when the app comes to the foreground and `false` when it goes to the background: when every window lost focus natively, or when the browser tab is hidden. Programs without windows stay active.

```dream
public static fun on_active(hook: fun(bool): void): Subscription
```

## `on_power_source`

Runs `hook` when the computer switches between wall power and battery.

```dream
public static fun on_power_source(hook: fun(PowerSource): void): Subscription
```

## `on_theme`

Runs `hook` when the user changes the system color scheme.

```dream
public static fun on_theme(hook: fun(SystemTheme): void): Subscription
```

## `on_locale`

Runs `hook` when the user changes the system language or regional formats, so cached formatted text can be rebuilt.

```dream
public static fun on_locale(hook: fun(): void): Subscription
```

## `on_low_memory`

Runs `hook` when the operating system reports that memory is running low. Drop caches that can be rebuilt. Repeated warnings that arrive together run the hook once.

```dream
public static fun on_low_memory(hook: fun(): void): Subscription
```

## `stopping`

A token that is cancelled once, when shutdown begins and before any `on_stop` cleanup runs. Loops check `is_cancelled`; waits take it as their `token` argument. Outside `App.run` the token is never cancelled.

```dream
public static get stopping(): CancellationToken
```

## `quit`

Stops the program with exit code `code`. `on_shutdown_requested` hooks see the reason `Requested`, which cannot be vetoed. Does nothing once shutdown has begun or when no `run` is active.

```dream
public static fun quit(code: int = 0): void
```

## `phase`

Where the program is in its lifetime. Intended for diagnostics and tests.

```dream
public static get phase(): AppPhase
```

## `is_session_locked`

True while the user session is locked or switched away from.

```dream
public static get is_session_locked(): bool
```

## `are_displays_on`

False while the displays are asleep. True when the platform cannot tell.

```dream
public static get are_displays_on(): bool
```

## `is_active`

True while the app is in the foreground. See `on_active`.

```dream
public static get is_active(): bool
```

## `power_source`

The current power source. Read when `run` starts and kept current afterwards.

```dream
public static get power_source(): PowerSource
```

## `theme`

The current system color scheme. Read when `run` starts and kept current afterwards.

```dream
public static get theme(): SystemTheme
```

## `simulate`

Delivers `event` as if the operating system had reported it, for tests. Hooks run before `simulate` returns. Returns false when the event was ignored: a duplicate (for example `Woke` while not asleep, or locking an already locked session), any event after shutdown began, or a vetoed shutdown request.

```dream
public static fun simulate(event: AppEvent): bool
```

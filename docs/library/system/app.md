---
title: App lifetime
summary: Start and stop a program cleanly, and react to sleep, session lock, power, theme and locale changes.
icon: material/power-settings
api: reference/api/system-app.md
---

# App lifetime

Import `system.app` and wrap `main` in `App.run`. Do startup in the function you pass, and register a cleanup right after you acquire each resource.

Ctrl+C, `SIGTERM`, `App.quit`, and an operating-system shutdown all stop the same way: cleanups run newest first, and `run` returns the exit code.

The same package reports sleep and wake, session lock, display power, backgrounding, power source, color scheme, locale, and low memory.

## Run a program

```dream
import system;
import system.app;

async fun heartbeat(): void {
    while !App.stopping.is_cancelled {
        System.println("tick");
        Time.delay(1000).await;
    }
}

async fun main(): int {
    return App.run(
        async () => {
            System.println("starting");
            App.on_stop(
                async () => {
                    System.println("releasing resources");
                }
            );
            heartbeat().await;
            return Result.Ok(Unit());
        }
    ).await;
}
```

This program prints `tick` every second. Pressing Ctrl+C cancels `App.stopping`, so the
loop ends after its current delay, `start` returns, the cleanup prints `releasing resources`, and
the process exits with code `130`.

`run` works in three steps:

1. **Startup.** It calls `start`, and hooks registered there are active immediately.
   If `start` returns `Err`, the error is printed as `error: CODE: message`, cleanups so far run, and `run` returns `1`.

2. **Running.** After `Ok`, the program runs until something asks it to stop.
   `start` may itself run until `App.stopping` is cancelled, for example a server.

3. **Shutdown.** `App.stopping` is cancelled first. `run` waits if `start` is still going, then runs `on_stop` cleanups newest first.

All of shutdown shares one budget, `shutdown_timeout_ms` (5000 ms by default), counted from the
moment shutdown begins. If `start` or a cleanup is still running when the budget is spent, it is
cancelled, a warning is printed, and the remaining cleanups still run.

| What stopped the program | Exit code |
| --- | --- |
| `App.quit(code)` | `code`, `0` by default |
| Ctrl+C (`SIGINT`) | `130` |
| `SIGTERM` | `143` |
| `start` returned `Err` | `1` |
| Operating-system shutdown or logout | `0` |

A second Ctrl+C or `SIGTERM` during shutdown exits immediately with that signal's code. Calling `run` while another `run` is active prints an error and returns `1`.

Outside `run`, `App.stopping` is never cancelled, so code that watches it also works without `run`.

`on_stop` returns `Err(AppError)` with code `ESTOPPING` once shutdown has begun, because a
cleanup registered then would never run. Every hook and cleanup returns a `Subscription`; call
`cancel` on it to remove the hook early, for example when the resource is released before
shutdown.

## Stop requests and vetoes

`on_shutdown_requested` runs when something asks the program to stop. The `ShutdownRequest`
says why, and lets a desktop program keep running, for example while a document has unsaved
changes:

```dream
App.on_shutdown_requested(
    (request) => {
        if request.can_veto && editor.has_unsaved_changes() {
            request.veto();
            editor.show_save_prompt();
        }
    }
);
```

| `ShutdownReason` | Sent when | Can veto |
| --- | --- | --- |
| `Interrupt` | Ctrl+C or `SIGINT` | No |
| `Terminate` | `SIGTERM` or `SIGHUP` on Unix; Ctrl+Break, closing the console, logout or shutdown for a Windows console program | No |
| `Requested` | The program called `App.quit` | No |
| `SessionEnding` | Windows: the user is logging out or the computer is shutting down. macOS: the computer is shutting down or restarting | Yes, on the first Windows notice |
| `OsQuit` | The app was asked to quit from its menu or Dock | Yes |
| `LastWindowClosed` | The program's last window closed | Yes |

On Windows, a veto of the first session-ending notice keeps the session alive. The final notice cannot be vetoed, and only the first accepted request starts shutdown.

No platform reports `OsQuit` or `LastWindowClosed` yet: Cmd+Q and closing a window currently
arrive as `WindowEvent.CloseRequested` on each window. Use them with `App.simulate` to
test vetoable shutdown paths.

## Operating-system events

Hooks run on the main scheduler, one event at a time, in registration order. They are not awaited, so keep them short and start a task for longer work.

A value that can change is also a property, filled in when `run` starts:

| Hook | Property | Reports |
| --- | --- | --- |
| `on_sleep` | `phase == AppPhase.Asleep` | The computer is about to sleep. Save quickly: the operating system waits at most about half a second. |
| `on_wake(slept_ms)` | | The computer woke after `slept_ms` milliseconds, or `-1` when unknown. Wall-clock time may have jumped. |
| `on_session_lock(locked)` | `is_session_locked` | The session was locked or another user switched in. Hide sensitive content. |
| `on_display_power(on)` | `are_displays_on` | The displays went to sleep or turned back on; the program keeps running. |
| `on_active(active)` | `is_active` | Every window lost focus or one regained it, or the browser tab was hidden or shown. |
| `on_power_source(source)` | `power_source` | The computer switched between `Ac` (wall power) and `Battery`. |
| `on_theme(theme)` | `theme` | The user switched between `Light`, `Dark` and `HighContrast`. |
| `on_locale()` | | The system language or regional formats changed. Rebuild cached formatted text. |
| `on_low_memory()` | | Memory is running low. Drop caches that can be rebuilt. |

This program pauses a sync loop while the computer sleeps and refreshes after waking:

```dream
import system;
import system.app;

async fun main(): int {
    return App.run(
        async () => {
            App.on_sleep(
                () => {
                    System.println("pausing sync");
                }
            );
            App.on_wake(
                (slept_ms) => {
                    System.println("slept for " + slept_ms.to_string() + " ms, refreshing");
                }
            );
            App.on_theme(
                (theme) => {
                    System.println("dark mode: " + (theme == SystemTheme.Dark).to_string());
                }
            );
            return Result.Ok(Unit());
        }
    ).await;
}
```

## Platform support

| Event | macOS | Windows | Linux | Browser | Node |
| --- | --- | --- | --- | --- | --- |
| Ctrl+C, `SIGTERM` | Yes | Yes | Yes | No | Yes |
| Logout or shutdown | Shutdown and restart | Yes, with veto | No | No | No |
| Sleep and wake | Yes | Yes | No | Page freeze and resume | No |
| Session lock | Yes | Yes | No | No | No |
| Display power | Yes | Yes | No | No | No |
| Foreground and background | Window focus | Window focus | Window focus | Tab visibility | No |
| Power source | Yes | Yes | No | No | No |
| Theme | Yes | Yes | No | Yes | No |
| Locale | Yes | Yes | No | Yes | No |
| Low memory | Yes | Yes | No | No | No |

On macOS, session lock, theme, and locale changes show up within about two seconds. Events a platform does not report never fire, and the property stays at its start value: unlocked, displays on, active, `PowerSource.Unknown`, and `SystemTheme.Light`.

## Test lifetime code

`App.simulate` delivers an `AppEvent` as if the operating system had reported it. Hooks
run before `simulate` returns, and it returns false when the event was ignored: a duplicate such
as `Woke` while not asleep, any event after shutdown began, or a vetoed shutdown request.

```dream
import system;
import system.app;
import system.testing;

@test
async fun pauses_while_asleep(): void {
    let code = App.run(
        async () => {
            let paused = false;
            App.on_sleep(
                () => {
                    paused = true;
                }
            );
            Assert.is_true(App.simulate(AppEvent.Sleeping));
            Assert.is_true(paused);
            Assert.is_true(App.phase == AppPhase.Asleep);
            Assert.is_false(App.simulate(AppEvent.Sleeping));
            App.quit(0);
            return Result.Ok(Unit());
        }
    ).await;
    Assert.eq(code, 0);
}
```

Use `App.quit` to end a test's `run`. When `run` returns, hooks are cleared and
`App.stopping` is a fresh token again, so the next test starts from a clean state.

See the [API reference](../../reference/api/system-app.md) for every declaration, and
[Lifecycle and state](../../web-server/lifecycle.md) for using `App.run` with a web server.

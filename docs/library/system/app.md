---
title: Application lifetime
summary: Start and stop a program cleanly, and react to sleep, session lock, power, theme and locale changes.
icon: material/power-settings
api: reference/api/system-app.md
---

# Application lifetime

Import `system.app` to give a program a clear startup and shutdown. Wrap `main` in
`Application.run`, do your startup in the function you pass to it, and register a cleanup for
each resource right after you acquire it. Ctrl+C, `SIGTERM`, `Application.quit` and an
operating-system shutdown then all stop the program the same way: cleanups run newest first and
`run` returns the exit code for `main` to return.

The same package reports what happens around the program: the computer going to sleep and
waking up, the user session being locked, displays turning off, the app moving to the
background, the power source and color scheme changing, and the system running low on memory.

## Run a program

```dream
import system;
import system.app;

async fun heartbeat(): void {
    while !Application.stopping.is_cancelled {
        System.println("tick");
        Time.delay(1000).await;
    }
}

async fun main(): int {
    return Application.run(
        async () => {
            System.println("starting");
            Application.on_stop(
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

This program prints `tick` every second. Pressing Ctrl+C cancels `Application.stopping`, so the
loop ends after its current delay, `start` returns, the cleanup prints `releasing resources`, and
the process exits with code `130`.

`run` works in three steps:

1. **Startup.** It calls `start`. Hooks and cleanups registered here are active straight away.
   If `start` returns `Err`, the error is printed to standard error as `error: CODE: message`,
   the cleanups registered so far run, and `run` returns `1`.
2. **Running.** After `start` returns `Ok`, the program keeps running until something asks it
   to stop. `start` may also keep running for the whole program, for example a web server's
   `run` call, as long as it returns once `Application.stopping` is cancelled.
3. **Shutdown.** `Application.stopping` is cancelled first. `run` waits for `start` if it is
   still running, then runs the `on_stop` cleanups in reverse registration order, so a resource
   is released before the resources it depends on.

All of shutdown shares one budget, `shutdown_timeout_ms` (5000 ms by default), counted from the
moment shutdown begins. If `start` or a cleanup is still running when the budget is spent, it is
cancelled, a warning is printed, and the remaining cleanups still run.

| What stopped the program | Exit code |
| --- | --- |
| `Application.quit(code)` | `code`, `0` by default |
| Ctrl+C (`SIGINT`) | `130` |
| `SIGTERM` | `143` |
| `start` returned `Err` | `1` |
| Operating-system shutdown or logout | `0` |

A second Ctrl+C or `SIGTERM` while shutdown is running ends the process immediately with that
signal's exit code. Calling `run` while another `run` is still active prints an error and returns `1`.
Outside `run`, `Application.stopping` is never cancelled, so code that watches it also works in
programs and tests that do not use `run`.

`on_stop` returns `Err(AppError)` with code `ESTOPPING` once shutdown has begun, because a
cleanup registered then would never run. Every hook and cleanup returns a `Subscription`; call
`cancel` on it to remove the hook early, for example when the resource is released before
shutdown.

## Stop requests and vetoes

`on_shutdown_requested` runs when something asks the program to stop. The `ShutdownRequest`
says why, and lets a desktop program keep running, for example while a document has unsaved
changes:

```dream
Application.on_shutdown_requested(
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
| `Requested` | The program called `Application.quit` | No |
| `SessionEnding` | Windows: the user is logging out or the computer is shutting down. macOS: the computer is shutting down or restarting | Yes, on the first Windows notice |
| `OsQuit` | The app was asked to quit from its menu or Dock | Yes |
| `LastWindowClosed` | The program's last window closed | Yes |

Windows asks running programs before the session ends, and a veto there keeps the session
alive. The final notice that the session is ending cannot be vetoed. Only the first accepted
request starts shutdown; later requests are ignored.

No platform reports `OsQuit` or `LastWindowClosed` yet: Cmd+Q and closing a window currently
arrive as `WindowEvent.CloseRequested` on each window. Use them with `Application.simulate` to
test vetoable shutdown paths.

## Operating-system events

Hooks run on the program's main scheduler, one event at a time, in registration order. They are
not awaited, so keep them short and start a task for longer work. Each event that also has a
current value is readable as a property, which is filled in when `run` starts:

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
    return Application.run(
        async () => {
            Application.on_sleep(
                () => {
                    System.println("pausing sync");
                }
            );
            Application.on_wake(
                (slept_ms) => {
                    System.println("slept for " + slept_ms.to_string() + " ms, refreshing");
                }
            );
            Application.on_theme(
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

On macOS, session lock, theme and locale changes are noticed within about two seconds. Events a
platform does not report never fire, and the matching property keeps its starting value:
unlocked, displays on, active, `PowerSource.Unknown` and `SystemTheme.Light`.

## Test lifetime code

`Application.simulate` delivers an `AppEvent` as if the operating system had reported it. Hooks
run before `simulate` returns, and it returns false when the event was ignored: a duplicate such
as `Woke` while not asleep, any event after shutdown began, or a vetoed shutdown request.

```dream
import system;
import system.app;
import system.testing;

@test
async fun pauses_while_asleep(): void {
    let code = Application.run(
        async () => {
            let paused = false;
            Application.on_sleep(
                () => {
                    paused = true;
                }
            );
            Assert.is_true(Application.simulate(AppEvent.Sleeping));
            Assert.is_true(paused);
            Assert.is_true(Application.phase == AppPhase.Asleep);
            Assert.is_false(Application.simulate(AppEvent.Sleeping));
            Application.quit(0);
            return Result.Ok(Unit());
        }
    ).await;
    Assert.eq(code, 0);
}
```

Use `Application.quit` to end a test's `run`. When `run` returns, hooks are cleared and
`Application.stopping` is a fresh token again, so the next test starts from a clean state.

See the [API reference](../../reference/api/system-app.md) for every declaration, and
[Lifecycle and state](../../web-server/lifecycle.md) for using `Application.run` with a web server.

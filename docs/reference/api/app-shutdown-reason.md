# Shutdown Reason

**Import:** `import system.app;`

Read the [usage guide](../../library/system/app.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `ShutdownReason`

Why the program is being asked to stop, as carried by `ShutdownRequest.reason`.

```dream
public enum ShutdownReason
```

## `Interrupt`

The user pressed Ctrl+C, or the process received `SIGINT`. It cannot be vetoed, and the exit code is `130`. Pressing Ctrl+C again while cleanups run exits immediately.

```dream
Interrupt
```

## `Terminate`

The process received `SIGTERM` or `SIGHUP`, or a Windows console program got Ctrl+Break, a console close, logoff or shutdown notice. It cannot be vetoed, and the exit code is `143`.

```dream
Terminate
```

## `OsQuit`

The user quit the app from its menu or Dock. It can be vetoed. No platform reports it yet: Cmd+Q currently arrives as `WindowEvent.CloseRequested` on each window, so deliver it with `Application.simulate` in tests.

```dream
OsQuit
```

## `SessionEnding`

On Windows, the user is logging off or the computer is shutting down; on macOS, the computer is shutting down or restarting. The first Windows request can be vetoed; the final notice cannot.

```dream
SessionEnding
```

## `LastWindowClosed`

The last open window of the app was closed. It can be vetoed. No platform reports it yet, so deliver it with `Application.simulate` in tests.

```dream
LastWindowClosed
```

## `Requested`

The program called `Application.quit`. It cannot be vetoed.

```dream
Requested
```

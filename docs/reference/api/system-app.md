# system.app

Import `system.app` to run a program with a clean startup and shutdown and to react to operating-system events such as sleep, session lock and theme changes. Read the [application lifetime guide](../../library/system/app.md) for examples and platform behavior.

| API | Purpose |
| --- | --- |
| [App](app-app.md) | Run the program, register shutdown cleanups and operating-system event hooks |
| [AppPhase](app-app-phase.md) | Tell whether the program is starting, running, asleep or stopping |
| [AppEvent](app-app-event.md) | Simulate lifetime events in tests |
| [ShutdownRequest](app-shutdown-request.md) | Inspect or veto a request to stop |
| [ShutdownReason](app-shutdown-reason.md) | Tell what asked the program to stop |
| [Subscription](app-subscription.md) | Remove a hook or cleanup |
| [PowerSource](app-power-source.md) | Tell whether the computer runs on mains power or battery |
| [SystemTheme](app-system-theme.md) | Read the light, dark or high-contrast appearance |
| [AppError](app-app-error.md) | Inspect application lifetime failures |

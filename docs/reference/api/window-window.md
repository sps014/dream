# Window

**Import:** `import system.window;`

Read the [usage guide](../../library/system/window.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Window`

A window, or a browser canvas, that receives keyboard, mouse and gamepad input.  Create one with `Window.open` in native programs, `Window.canvas` in browsers, or `Window.headless` for tests and simulations without a display. Call `poll` once per frame: it pumps the platform's events and returns an `InputFrame`.  Native windows belong to the program's main thread, which is where `main` and its `async` code run. Calls from a `Task` worker fail or do nothing. In browsers the page only delivers input while the program waits, so a frame loop must `await` something every frame, such as presenting a GPU frame or `Time.delay(16)`.

```dream
public class Window
```

## `open`

Opens a native window described by `desc` and shows it.  In a browser this creates a `<canvas>` of the requested size at the end of the page and uses the title as the page title. Under Node, on a machine without a display, or when called from a `Task` worker, it returns a `WindowError`; see `WindowError.code` for the reasons.

```dream
public static fun open(desc: WindowDesc): Result<Window, WindowError>
```

## `canvas`

Uses the page's `<canvas>` element whose `id` attribute is `id` as a window.  The page keeps control of the canvas's layout size; the window follows it and reports changes as `WindowEvent.Resized`. The canvas becomes focusable so it can receive keyboard input after the user clicks it. Returns a `WindowError` with code `ENOTFOUND` when no such canvas exists.

```dream
public static fun canvas(id: string): Result<Window, WindowError>
```

## `headless`

Creates a window that exists only in memory. It never appears on screen and receives only the events passed to `simulate`, so tests, replays and servers can run input-driven code on machines without a display. Its pixel size equals its logical size, and simulated `Resized` events change both.

```dream
public static fun headless(desc: WindowDesc): Window
```

## `is_open`

False once `close` was called or a poll delivered `WindowEvent.CloseRequested`. After a close request the native window stays on screen until `close` is called or the `Window` is released, so the current frame can still finish drawing.

```dream
public get is_open(): bool
```

## `title`

The title last set by `WindowDesc.of` or `set_title`. Canvas windows start with an empty title.

```dream
public get title(): string
```

## `is_fullscreen`

True while the window covers a monitor in fullscreen mode.

```dream
public get is_fullscreen(): bool
```

## `is_focused`

True while the window has keyboard focus.

```dream
public get is_focused(): bool
```

## `size`

The content area's size in logical pixels, the units of `WindowDesc.of`, `set_size` and mouse positions.

```dream
public get size(): WindowSize
```

## `pixel_size`

The drawable area's size in physical pixels, the size a GPU surface needs. It matches the size carried by the most recent `WindowEvent.Resized`.

```dream
public get pixel_size(): WindowSize
```

## `scale_factor`

Physical pixels per logical pixel on the window's current monitor: `1.0` on standard displays and `2.0` on most high-DPI displays.

```dream
public get scale_factor(): float
```

## `cursor_mode`

The cursor mode last set with `set_cursor_mode`; starts as `CursorMode.Normal`.

```dream
public get cursor_mode(): CursorMode
```

## `poll`

Pumps the platform's pending events and returns everything that happened since the previous poll. Call it once per frame. Events queued with `simulate` follow the platform's events. A closed window still returns frames, with no platform events.

```dream
public fun poll(): InputFrame
```

## `simulate`

Queues `event` as if the platform had reported it. The next `poll` delivers it after the platform's own events and updates the keyboard and mouse state from it. Use it for automated tests, input replays and on-screen controls. Simulated events never move the real cursor or resize the real window.

```dream
public fun simulate(event: WindowEvent): void
```

## `close`

Closes the window and removes it from the screen. Closing twice does nothing.

```dream
public fun close(): void
```

## `set_title`

Changes the title bar text, or the page title for a window made by `open` in a browser.

```dream
public fun set_title(title: string): void
```

## `set_size`

Resizes the content area to `width` by `height` logical pixels. A `WindowEvent.Resized` follows once the platform applied the change. In browsers this sets the canvas's CSS size, so page styles that fix the canvas size still win. Non-positive sizes are ignored.

```dream
public fun set_size(width: int, height: int): void
```

## `set_fullscreen`

Enters or leaves fullscreen. Native windows cover the primary monitor at its current video mode. Browsers only allow fullscreen during a user gesture, such as a key or click handler's frame, and may refuse it.

```dream
public fun set_fullscreen(enabled: bool): void
```

## `set_cursor_mode`

Shows, hides or captures the cursor while it is over the window.

```dream
public fun set_cursor_mode(mode: CursorMode): void
```

## `set_cursor_icon`

Changes the cursor's shape while it is over the window. Shapes the platform does not provide fall back to the default arrow.

```dream
public fun set_cursor_icon(icon: CursorIcon): void
```

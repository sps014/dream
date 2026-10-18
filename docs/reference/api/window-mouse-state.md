# MouseState

**Import:** `import system.window;`

Read the [usage guide](../../library/system/window.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `MouseState`

The mouse as of one `Window.poll`.  Button queries follow the same rules as `KeyboardState`: `is_down` is the level, `pressed` and `released` are true only in the frame where the button changed.

```dream
public class MouseState
```

## `position`

The last cursor position in window coordinates. While the cursor mode is `CursorMode.Captured` this is a virtual position that keeps moving without bounds; use `delta` for camera controls.

```dream
public get position(): WindowVector
```

## `delta`

How far the cursor moved during this frame, in window coordinates. The first movement after the window opens reports no motion.

```dream
public get delta(): WindowVector
```

## `scroll`

The total scroll during this frame. One wheel notch is `1.0`; positive `y` scrolls up and positive `x` scrolls right.

```dream
public get scroll(): WindowVector
```

## `is_down`

True while `button` is held down.

```dream
public fun is_down(button: MouseButton): bool
```

## `pressed`

True when `button` went down during this frame.

```dream
public fun pressed(button: MouseButton): bool
```

## `released`

True when `button` was released during this frame.

```dream
public fun released(button: MouseButton): bool
```

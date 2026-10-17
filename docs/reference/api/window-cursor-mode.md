# CursorMode

**Import:** `import system.window;`

Read the [usage guide](../../library/system/window.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `CursorMode`

How the mouse cursor behaves over a window.

```dream
public enum CursorMode
```

## `Normal`

The cursor is visible and moves freely.

```dream
Normal = 0
```

## `Hidden`

The cursor is invisible while it is over the window but is not confined.

```dream
Hidden = 1
```

## `Captured`

The cursor is hidden and locked to the window, for first-person camera controls. `MouseState.delta` keeps reporting movement without limit. Browsers only grant the lock after a user gesture, such as a click on the canvas.

```dream
Captured = 2
```

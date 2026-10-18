# KeyboardState

**Import:** `import system.window;`

Read the [usage guide](../../library/system/window.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `KeyboardState`

The keyboard as of one `Window.poll`.  `is_down` is the current level of a key. `pressed` and `released` are edges: they are true only in the frame where the key changed, and a key pressed and released between two polls reports both edges in the same frame while `is_down` is false. Key repeats never count as presses.

```dream
public class KeyboardState
```

## `is_down`

True while `code` is held down.

```dream
public fun is_down(code: KeyCode): bool
```

## `pressed`

True when `code` went down during this frame.

```dream
public fun pressed(code: KeyCode): bool
```

## `released`

True when `code` was released during this frame.

```dream
public fun released(code: KeyCode): bool
```

# Gamepads

**Import:** `import system.window;`

Read the [usage guide](../../library/system/window.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Gamepads`

The gamepads connected as of one `Window.poll`, ordered by slot.

```dream
public class Gamepads
```

## `length`

The number of connected gamepads.

```dream
public get length(): int
```

## `first`

The connected gamepad with the lowest slot, or `None` when none is connected.

```dream
public fun first(): Option<Gamepad>
```

## `at`

The connected gamepad at `index`, counting from `0` in slot order, or `None` when `index` is negative or not less than `length`.

```dream
public fun at(index: int): Option<Gamepad>
```

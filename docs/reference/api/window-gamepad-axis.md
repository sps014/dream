# GamepadAxis

**Import:** `import system.window;`

Read the [usage guide](../../library/system/window.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `GamepadAxis`

An analog control on a standard-layout gamepad.  Stick axes range from `-1.0` to `1.0`; positive X is right and positive Y is down. Trigger axes range from `0.0` (released) to `1.0` (fully pressed).

```dream
public enum GamepadAxis
```

## `LeftX`

Horizontal position of the left stick.

```dream
LeftX = 0
```

## `LeftY`

Vertical position of the left stick.

```dream
LeftY = 1
```

## `RightX`

Horizontal position of the right stick.

```dream
RightX = 2
```

## `RightY`

Vertical position of the right stick.

```dream
RightY = 3
```

## `LeftTrigger`

How far the left trigger is pressed.

```dream
LeftTrigger = 4
```

## `RightTrigger`

How far the right trigger is pressed.

```dream
RightTrigger = 5
```

# GamepadButton, GamepadAxis

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](gpu-gpu-gamepad.md)

## `RightTrigger = 5`

Right trigger analog (0…1).

```dream
RightTrigger = 5
```

## `gamepad_button_from_id`

Map a host button id to `GamepadButton`.

```dream
public fun gamepad_button_from_id(id: int): GamepadButton
```

## `gamepad_axis_from_id`

Map a host axis id to `GamepadAxis` (defaults to `LeftStickX` for out-of-range).

```dream
public fun gamepad_axis_from_id(id: int): GamepadAxis
```

## `gamepad_button_id`

Wire discriminant for a button (for host latch calls).

```dream
public fun gamepad_button_id(button: GamepadButton): int
```

## `gamepad_axis_id`

Wire discriminant for an axis (for host latch calls).

```dream
public fun gamepad_axis_id(axis: GamepadAxis): int
```

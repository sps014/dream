# GamepadButton, GamepadAxis

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](gpu-gpu-gamepad.md)

## `enum GamepadButton`

Standard gamepad face / D-pad / shoulder / menu buttons (Xbox layout names). Discriminants 0… are shared with the host wire format — keep in sync with `gpu/input.rs`.

```dream
public enum GamepadButton
```

## `Unknown = 0`

Unrecognized host button id.

```dream
Unknown = 0
```

## `South = 1`

Bottom face button (Xbox A / DualShock ×).

```dream
South = 1
```

## `East = 2`

Right face button (Xbox B / DualShock ○).

```dream
East = 2
```

## `West = 3`

Left face button (Xbox X / DualShock □).

```dream
West = 3
```

## `North = 4`

Top face button (Xbox Y / DualShock △).

```dream
North = 4
```

## `DPadUp = 5`

D-pad up.

```dream
DPadUp = 5
```

## `DPadDown = 6`

D-pad down.

```dream
DPadDown = 6
```

## `DPadLeft = 7`

D-pad left.

```dream
DPadLeft = 7
```

## `DPadRight = 8`

D-pad right.

```dream
DPadRight = 8
```

## `LeftShoulder = 9`

Left bumper.

```dream
LeftShoulder = 9
```

## `RightShoulder = 10`

Right bumper.

```dream
RightShoulder = 10
```

## `LeftTrigger = 11`

Left analog trigger (as a button latch).

```dream
LeftTrigger = 11
```

## `RightTrigger = 12`

Right analog trigger (as a button latch).

```dream
RightTrigger = 12
```

## `LeftStick = 13`

Click of the left stick.

```dream
LeftStick = 13
```

## `RightStick = 14`

Click of the right stick.

```dream
RightStick = 14
```

## `Start = 15`

Start / Options / Menu.

```dream
Start = 15
```

## `Select = 16`

Select / Share / Back.

```dream
Select = 16
```

## `enum GamepadAxis`

Analog axes (sticks −1…1, triggers 0…1 after host deadzone).

```dream
public enum GamepadAxis
```

## `LeftStickX = 0`

Left stick horizontal (−1 left … 1 right).

```dream
LeftStickX = 0
```

## `LeftStickY = 1`

Left stick vertical (−1 up … 1 down, host convention).

```dream
LeftStickY = 1
```

## `RightStickX = 2`

Right stick horizontal.

```dream
RightStickX = 2
```

## `RightStickY = 3`

Right stick vertical.

```dream
RightStickY = 3
```

## `LeftTrigger = 4`

Left trigger analog (0…1).

```dream
LeftTrigger = 4
```

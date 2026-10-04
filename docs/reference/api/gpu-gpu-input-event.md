# GpuInputEvent

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `enum GpuInputEvent`

Queued input events drained by `GpuSurface.poll_events` (web + native).

```dream
public enum GpuInputEvent
```

## `PointerDown(x: float, y: float, button: int, pointer_id: int)`

Pointer button pressed (client pixels).

```dream
PointerDown(x: float, y: float, button: int, pointer_id: int)
```

## `PointerUp(x: float, y: float, button: int, pointer_id: int)`

Pointer button released.

```dream
PointerUp(x: float, y: float, button: int, pointer_id: int)
```

## `PointerMove(x: float, y: float, pointer_id: int)`

Pointer moved while tracked.

```dream
PointerMove(x: float, y: float, pointer_id: int)
```

## `PointerEnter(x: float, y: float, pointer_id: int)`

Pointer entered the surface.

```dream
PointerEnter(x: float, y: float, pointer_id: int)
```

## `PointerLeave(x: float, y: float, pointer_id: int)`

Pointer left the surface.

```dream
PointerLeave(x: float, y: float, pointer_id: int)
```

## `PointerCancel(id:int)`

Pointer capture cancelled by the host.

```dream
PointerCancel(id:int)
```

## `Wheel(dx: float, dy: float, x: float, y: float)`

Mouse wheel / trackpad scroll deltas.

```dream
Wheel(dx: float, dy: float, x: float, y: float)
```

## `KeyDown(code: KeyCode, key: string, repeat: bool)`

`code` is physical; `key` is the typed glyph when available.

```dream
KeyDown(code: KeyCode, key: string, repeat: bool)
```

## `KeyUp(code: KeyCode, key: string)`

Key released (physical `code`).

```dream
KeyUp(code: KeyCode, key: string)
```

## `TextInput(string)`

Composed text input (IME / dead keys).

```dream
TextInput(string)
```

## `Resize(width: int, height: int)`

Surface client size changed (logical pixels).

```dream
Resize(width: int, height: int)
```

## `ScaleFactor(float)`

Device pixel ratio / window scale factor changed.

```dream
ScaleFactor(float)
```

## `Focus`

Surface gained keyboard focus.

```dream
Focus
```

## `Blur`

Surface lost keyboard focus.

```dream
Blur
```

## `Close`

User asked to close the window (native).

```dream
Close
```

## `GamepadConnected(int)`

Gamepad at slot `pad` connected.

```dream
GamepadConnected(int)
```

## `GamepadDisconnected(int)`

Gamepad at slot `pad` disconnected.

```dream
GamepadDisconnected(int)
```

## `GamepadButtonDown(pad: int, button: GamepadButton)`

Gamepad button pressed.

```dream
GamepadButtonDown(pad: int, button: GamepadButton)
```

## `GamepadButtonUp(pad: int, button: GamepadButton)`

Gamepad button released.

```dream
GamepadButtonUp(pad: int, button: GamepadButton)
```

## `GamepadAxis(pad: int, axis: GamepadAxis, value: float)`

Analog axis changed after host deadzone (`GamepadAxis` wire id).

```dream
GamepadAxis(pad: int, axis: GamepadAxis, value: float)
```

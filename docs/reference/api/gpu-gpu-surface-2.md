# GpuSurface

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](gpu-gpu-surface.md)

## `gamepad_button_down`

True while a gamepad button is held past the host threshold.

```dream
public fun gamepad_button_down(pad: int, button: GamepadButton): bool
```

## `gamepad_axis`

Stick axes ~−1…1; triggers 0…1 (host deadzone applied).

```dream
public fun gamepad_axis(pad: int, axis: GamepadAxis): float
```

## `focused`

True when the surface/window has keyboard focus.

```dream
public get focused(): bool
```

## `close_requested`

Sticky latch set on window close (native) or sample Escape handling.

```dream
public get close_requested(): bool
```

## `poll_events`

Drain queued input events since the last call.

```dream
public fun poll_events(): GpuInputEvent[]
```

## `destroy`

Releases the host surface / window resource.

```dream
public fun destroy(): void
```

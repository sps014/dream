# Read window and device input

Use a GpuSurface to draw to a desktop window or browser canvas. Read its input state to react to pointers, keys, and gamepads. Create and configure the surface before reading it.

### Surfaces and input

- Swapchain drawable size is CSS/logical pixels unless `GpuSurfaceDesc.max_pixel_ratio` is greater than `1`: then `width`/`height` become `client × min(devicePixelRatio, max_pixel_ratio)` (typical game clamp is `2`).
- Read the used scale with `surface.pixel_ratio` and the uncapped window/DPR with `surface.scale_factor`.
- `request_pointer_lock()` feeds relative `dx`/`dy` for FPS cameras; `request_fullscreen()` is borderless. Both need a user gesture in the browser.
- `pointers()` is the multi-touch list (`pointer()` stays the primary latch).
- Gamepad sticks still poll via `gamepad_axis`; `poll_events` also yields `GamepadAxis` when a value changes.


## Input types

`GpuPointer` reports pointer data, `GpuMods` modifier-key state, and `KeyCode` named keys. Gamepad input uses `GamepadButton` and `GamepadAxis`. `GpuInputEvent` represents input event data.

Use `surface.key_down(code)` for a key's current state and `surface.mods()` for modifiers. `gamepads()` lists connected device IDs; check `gamepad_connected(id)` before reading one. Browser pointer-lock and fullscreen requests need a user gesture.

See [surface methods](../api/gpu-gpu-surface.md), [pointer fields](../api/gpu-gpu-pointer.md), [events](../api/gpu-gpu-input-event.md), [key choices](../api/gpu-gpu-key-code.md), and [gamepad choices](../api/gpu-gpu-gamepad.md). Return to the [GPU overview](gpu.md) for setup and drawing guides.

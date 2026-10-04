# GpuSurface

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](gpu-gpu-surface.md)

## `class GpuSurface`

Canvas / native-window swapchain + polled input (web and native).

```dream
public class GpuSurface
```

## `id`

```dream
public get id(): int
```

## `from_canvas`

Creates a surface bound to the DOM canvas with id `canvas_id` (browser), or a native window titled `canvas_id` under `dream run` (default 800×600).

```dream
public static fun from_canvas(canvas_id: string): Result<GpuSurface, GpuError>
```

## `from_canvas`

Like `from_canvas(id)`, with an initial swapchain / window size.

```dream
public static fun from_canvas(canvas_id: string, width: int, height: int): Result<GpuSurface, GpuError>
```

## `create`

Native-friendly alias: window titled `title` at `width`×`height` (same host as `from_canvas`).

```dream
public static fun create(title: string, width: int, height: int): Result<GpuSurface, GpuError>
```

## `configure`

Sets the window client size (logical) and rebuilds the drawable swapchain.

```dream
public fun configure(width: int, height: int): void
```

## `configure`

Like `configure(w, h)`, with present mode, alpha, color space, and a pixel-ratio clamp.

```dream
public fun configure(desc: GpuSurfaceDesc): void
```

## `width`

Drawable width in pixels (`canvas.width` on web). Equals the client size when `max_pixel_ratio` is `1`; otherwise `client × min(DPR, max_pixel_ratio)`.

```dream
public get width(): int
```

## `height`

Drawable height in pixels.

```dream
public get height(): int
```

## `present`

Presents the current backbuffer to the canvas / native window.

```dream
public async fun present(token: Option<CancellationToken> = Option.None): Result<bool, GpuError>
```

## `pointer`

Latched pointer state; clears `dx`/`dy` after each read.

```dream
public fun pointer(): GpuPointer
```

## `pointers`

Every currently tracked pointer (mouse, pen, extra fingers). Clears per-pointer `dx`/`dy`. Prefer this over `pointer()` for pinch / look-plus-move; don't mix both in one frame if you need the primary deltas.

```dream
public fun pointers(): GpuPointer[]
```

## `pixel_ratio`

Backing-store scale actually used (`width / client`, 1 when HiDPI is clamped off).

```dream
public get pixel_ratio(): float
```

## `scale_factor`

Raw window scale / `devicePixelRatio` (uncapped).

```dream
public get scale_factor(): float
```

## `request_pointer_lock`

Hide the cursor and report relative `dx`/`dy` (FPS cameras). Needs a user gesture on the web.

```dream
public fun request_pointer_lock(): void
```

## `exit_pointer_lock`

```dream
public fun exit_pointer_lock(): void
```

## `pointer_locked`

```dream
public get pointer_locked(): bool
```

## `request_fullscreen`

Borderless fullscreen. Needs a user gesture on the web.

```dream
public fun request_fullscreen(): void
```

## `exit_fullscreen`

```dream
public fun exit_fullscreen(): void
```

## `fullscreen`

```dream
public get fullscreen(): bool
```

## `mods`

Latched modifier keys.

```dream
public fun mods(): GpuMods
```

## `key_down`

True while a physical key is held.

```dream
public fun key_down(code: KeyCode): bool
```

## `gamepads`

Connected gamepad indices (ascending); prefer over scanning a fixed range.

```dream
public fun gamepads(): int[]
```

## `gamepad_connected`

True when `pad` is currently connected.

```dream
public fun gamepad_connected(pad: int): bool
```

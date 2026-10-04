# GpuRenderTarget

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class GpuRenderTarget`

Attachment set for one render pass: up to N color targets (MRT) plus optional depth/stencil. ```dream let target = GpuRenderTarget.surface(surface) .clear(GpuVec4.of(0.05, 0.06, 0.08, 1.0)) .surface_depth(GpuLoadOp.Clear); ```

```dream
public class GpuRenderTarget
```

## `surface`

Renders into the surface swapchain.

```dream
public static fun surface(s: GpuSurface): GpuRenderTarget
```

## `texture`

Renders into an offscreen color texture.

```dream
public static fun texture(t: GpuTexture): GpuRenderTarget
```

## `color`

Appends another color attachment (MRT). The fragment shader must declare a matching `@location(N)` output for each.

```dream
public fun color(t: GpuTexture): GpuRenderTarget
```

## `clear`

Clears the most recently added attachment to `c`.

```dream
public fun clear(c: GpuVec4): GpuRenderTarget
```

## `load`

Preserves the existing contents of the most recently added attachment.

```dream
public fun load(): GpuRenderTarget
```

## `discard`

Discards the most recently added attachment after the pass instead of storing it.

```dream
public fun discard(): GpuRenderTarget
```

## `resolve`

MSAA resolve destination for the most recently added attachment.

```dream
public fun resolve(t: GpuTexture): GpuRenderTarget
```

## `depth`

Attaches an explicit depth texture.

```dream
public fun depth(t: GpuTexture, op: GpuLoadOp): GpuRenderTarget
```

## `surface_depth`

Attaches the depth texture the surface manages itself (sized to the swapchain).

```dream
public fun surface_depth(op: GpuLoadOp): GpuRenderTarget
```

## `depth_clear`

Depth clear value (default `1.0`).

```dream
public fun depth_clear(v: float): GpuRenderTarget
```

## `depth_discard`

Discards depth after the pass (the common case — nothing samples it later).

```dream
public fun depth_discard(): GpuRenderTarget
```

## `stencil`

Stencil load/store ops and clear value.

```dream
public fun stencil(load: GpuLoadOp, store: GpuStoreOp, clear_value: int): GpuRenderTarget
```

## `timestamps`

Writes GPU timestamps at `begin_index` / `end_index` of `queries` for this pass. Pass `-1` for an index to skip that write. Requires `timestamp_query`.

```dream
public fun timestamps(queries: GpuQuerySet, begin_index: int, end_index: int): GpuRenderTarget
```

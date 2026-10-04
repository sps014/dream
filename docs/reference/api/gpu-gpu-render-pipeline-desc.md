# GpuRenderPipelineDesc

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `struct GpuRenderPipelineDesc`

Fixed-function state for `GpuRenderPipeline.create_ex`. Topology: `0` triangle-list (default), `1` triangle-strip, `2` line-list, `3` line-strip, `4` point-list. Cull: `0` none, `1` front, `2` back. Front face: `0` ccw, `1` cw. Depth compare: `0` less, `1` less-equal, `2` greater, `3` greater-equal, `4` always, `5` never.

```dream
public struct GpuRenderPipelineDesc
```

## `topology: int`

Primitive topology code (see type comment).

```dream
public topology: int
```

## `cull_mode: int`

Face culling mode (see type comment).

```dream
public cull_mode: int
```

## `front_face: int`

Front-face winding (see type comment).

```dream
public front_face: int
```

## `depth_enabled: bool`

When true, depth testing is enabled.

```dream
public depth_enabled: bool
```

## `depth_write: bool`

When true, passing fragments write depth.

```dream
public depth_write: bool
```

## `depth_compare: int`

Depth compare function code (see type comment).

```dream
public depth_compare: int
```

## `blend_enabled: bool`

When true, alpha blending is enabled on the color target.

```dream
public blend_enabled: bool
```

## `sample_count: int`

MSAA sample count (`1` = no multisample).

```dream
public sample_count: int
```

## `mesh`

Defaults suitable for opaque triangle meshes with depth testing.

```dream
public static fun mesh(): GpuRenderPipelineDesc
```

## `overlay`

Defaults for unlit UI / overlays (no depth, alpha blending).

```dream
public static fun overlay(): GpuRenderPipelineDesc
```

## `defaults`

Bare triangle-list defaults (matches legacy `GpuRenderPipeline.create`).

```dream
public static fun defaults(): GpuRenderPipelineDesc
```

# GpuHitGroup

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuHitGroup`

The shaders that handle rays hitting one kind of geometry: a `@closest_hit` shader, plus optionally an `@any_hit` shader and, for custom shapes, an `@intersection` shader.  Start from `GpuHitGroup.closest` and add the optional shaders.

```dream
public class GpuHitGroup
```

## `closest`

A hit group that runs `shader`, a `@closest_hit` shader's `GpuShaders` entry, for the nearest hit.

```dream
public static fun closest(shader: GpuStageShader): GpuHitGroup
```

## `with_any_hit`

Adds an `@any_hit` shader, and returns this hit group.

```dream
public fun with_any_hit(shader: GpuStageShader): GpuHitGroup
```

## `with_intersection`

Adds an `@intersection` shader for custom shapes built with `GpuGeometry.boxes`, and returns this hit group.

```dream
public fun with_intersection(shader: GpuStageShader): GpuHitGroup
```

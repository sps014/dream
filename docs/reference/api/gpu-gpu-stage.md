# GpuStage

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuStage`

The pipeline stage a shader runs in, as given by its attribute.

```dream
public enum GpuStage
```

## `Compute`

A `@compute` shader.

```dream
Compute = 0
```

## `Vertex`

A `@vertex` shader.

```dream
Vertex = 1
```

## `Fragment`

A `@fragment` shader.

```dream
Fragment = 2
```

## `RayGeneration`

A `@ray_generation` shader.

```dream
RayGeneration = 3
```

## `ClosestHit`

A `@closest_hit` shader.

```dream
ClosestHit = 4
```

## `AnyHit`

An `@any_hit` shader.

```dream
AnyHit = 5
```

## `Miss`

A `@miss` shader.

```dream
Miss = 6
```

## `Intersection`

An `@intersection` shader.

```dream
Intersection = 7
```

## `Callable`

A `@callable` shader.

```dream
Callable = 8
```

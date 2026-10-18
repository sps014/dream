# GpuRenderPassDesc

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuRenderPassDesc`

The color and depth targets of a render pass, and how each starts.  Start from `GpuRenderPassDesc.color` and add a depth buffer with `with_depth`.

```dream
public class GpuRenderPassDesc
```

## `color`

A pass that draws into `target`, which starts as `load` says.

```dream
public static fun color(target: GpuTexture, load: GpuLoad<GpuVec4>): GpuRenderPassDesc
```

## `with_depth`

Adds `depth` as the pass's depth buffer, which starts as `load` says, and returns this description. The texture needs a depth format and must be a render target.

```dream
public fun with_depth(depth: GpuTexture, load: GpuLoad<float>): GpuRenderPassDesc
```

# GpuInstance

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside shader entry points and helpers reached from them; using them elsewhere is a compile error.

## `GpuInstance`

One placed copy of a bottom-level acceleration structure inside a top-level one.  Start from `GpuInstance(...)` and chain the settings you want to change.

```dream
public class GpuInstance
```

## `constructor`

Places a bottom-level acceleration structure with the supplied object-to-world transform.

```dream
public constructor(bottom_level: GpuAccelerationStructure, transform: GpuMat4)
```

## `with_id`

Sets the value hit shaders read with `hit_instance_id()`, and returns this instance.

```dream
public fun with_id(id: int): GpuInstance
```

## `with_mask`

Sets the 8-bit visibility mask, and returns this instance. A ray only sees instances whose mask shares a bit with the ray's mask. Defaults to `255`.

```dream
public fun with_mask(mask: int): GpuInstance
```

## `with_hit_group`

Sets which hit group of the ray tracing pipeline handles hits on this instance, and returns this instance. Defaults to `0`.

```dream
public fun with_hit_group(index: int): GpuInstance
```

# GpuDispatchIndirect

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `struct GpuDispatchIndirect`

Indirect dispatch workgroup counts (12 bytes / 3×u32) for `Compute.dispatch_indirect`. Values are workgroup counts (not thread extents).

```dream
public struct GpuDispatchIndirect
```

## `x: int`

Workgroup count in X.

```dream
public x: int
```

## `y: int`

Workgroup count in Y.

```dream
public y: int
```

## `z: int`

Workgroup count in Z.

```dream
public z: int
```

## `to_buffer`

Builds an indirect-args buffer of three i32 workgroup counts.

```dream
public fun to_buffer(): GpuBuffer<int>
```

## `write_to`

Writes workgroup counts into an existing buffer at element offset 0.

```dream
public fun write_to(buf: GpuBuffer<int>): void
```

# GpuSwap

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class GpuSwap<T : unmanaged>`

Ping-pong pair of storage buffers.

```dream
public class GpuSwap<T : unmanaged>
```

## `alloc`

Allocates two buffers of `n` elements; `front()` starts as `a`.

```dream
public static fun alloc(n: int): GpuSwap<T>
```

## `front`

The buffer currently written as the primary target.

```dream
public fun front(): GpuBuffer<T>
```

## `back`

The other buffer (typically last frame / read source).

```dream
public fun back(): GpuBuffer<T>
```

## `swap`

Swaps front and back roles.

```dream
public fun swap(): void
```

# GpuBindList

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class GpuBindList`

Builder for mixed compute bindings (buffers / textures / samplers) without raw id arrays.

```dream
public class GpuBindList
```

## `begin`

Starts an empty binding list.

```dream
public static fun begin(): GpuBindList
```

## `buffer`

```dream
public fun buffer<T : unmanaged>(buf: GpuBuffer<T>): GpuBindList
```

## `texture`

```dream
public fun texture(tex: GpuTexture): GpuBindList
```

## `sampler`

```dream
public fun sampler(samp: GpuSampler): GpuBindList
```

## `buffer_ids`

Snapshot of buffer ids for host dispatch.

```dream
public fun buffer_ids(): int[]
```

## `texture_ids`

Snapshot of texture ids for host dispatch.

```dream
public fun texture_ids(): int[]
```

## `sampler_ids`

Snapshot of sampler ids for host dispatch.

```dream
public fun sampler_ids(): int[]
```

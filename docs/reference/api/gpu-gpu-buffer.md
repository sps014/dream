# GpuBuffer

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `struct GpuBuffer<T : unmanaged>`

Value-typed handle (id / length / stride); device memory lives on the host/WebGPU side.

```dream
public struct GpuBuffer<T : unmanaged>
```

## `length`

Number of elements this buffer holds.

```dream
public get length(): int
```

## `id`

Host-side buffer handle id.

```dream
public get id(): int
```

## `stride`

Byte size of one `T` element.

```dream
public get stride(): int
```

## `alloc`

Allocates an uninitialized GPU buffer of `n` elements.

```dream
public static fun alloc(n: int): GpuBuffer<T>
```

## `from`

Allocates a buffer and uploads `data`.

```dream
public static fun from(data: T[]): GpuBuffer<T>
```

## `vertex`

Allocates an uninitialized GPU vertex buffer of `n` elements (`VERTEX  /  COPY_DST`).

```dream
public static fun vertex(n: int): GpuBuffer<T>
```

## `vertex_from`

Allocates a vertex buffer and uploads `data`.

```dream
public static fun vertex_from(data: T[]): GpuBuffer<T>
```

## `write`

Overwrites the whole buffer from CPU `data` (must match `length`).

```dream
public fun write(data: T[]): void
```

## `write_at`

Partial CPU→GPU write starting at element `offset`.

```dream
public fun write_at(offset: int, data: T[]): void
```

## `read`

Reads the entire buffer back to a CPU array.

```dream
public async fun read(token: Option<CancellationToken> = Option.None): T[]
```

## `read_at`

Reads `count` elements starting at element `offset`.

```dream
public async fun read_at(offset: int, count: int, token: Option<CancellationToken> = Option.None): T[]
```

## `copy_to`

GPU-side copy of `count` elements into `dst` (no CPU round-trip).

```dream
public fun copy_to(dst: GpuBuffer<T>, src_offset: int, dst_offset: int, count: int): void
```

## `destroy`

Releases the host buffer resource.

```dream
public fun destroy(): void
```

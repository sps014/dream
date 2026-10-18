# GpuBuffer

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuBuffer`

An array of `T` in GPU memory.  Create one with `GpuDevice.buffer_from`, `buffer_alloc`, `vertex_buffer_from` or `index_buffer_from`, and pass it to shaders. Inside a shader, `buffer[i]` reads and writes element `i`; a parameter marked `@readonly` can only be read. On the CPU, `write` uploads new contents and `read` downloads them after the GPU finished. `T` must be a type without references, such as `float`, `int`, `GpuVec4` or a struct of them.

```dream
public class GpuBuffer<T: unmanaged>
```

## `length`

The number of elements.

```dream
public get length(): int
```

## `write`

Replaces the whole contents with `data`, which must have exactly `length` elements.

```dream
public fun write(data: T[]): Result<bool, GpuError>
```

## `write_at`

Replaces the elements starting at `offset` with `data`. The range must lie inside the buffer. On `GpuBackend.WebGpu` it must also start at a multiple of 4 bytes and end at one or at the end of the buffer, which matters for `float16` and `byte` elements.

```dream
public fun write_at(offset: int, data: T[]): Result<bool, GpuError>
```

## `read`

Downloads every element. It waits for submitted GPU work that writes the buffer.

```dream
public async fun read(): Result<T[], GpuError>
```

## `destroy`

Releases the GPU memory as soon as submitted work that uses it has finished. Later uses return `GpuError.InvalidResource`. Releasing the last reference does the same.

```dream
public fun destroy(): void
```

## `this`

Reads element `index` inside a shader. Indexes outside the buffer read zero on most backends; never rely on it.

```dream
public extern fun this[index: int]: T
```

## `this`

Writes element `index` inside a shader. Not allowed on `@readonly` parameters. Writes outside the buffer are dropped.

```dream
public extern fun this[index: int] = value: T
```

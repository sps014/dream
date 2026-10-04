# GPU buffers

Buffers hold the data read or written by your GPU functions. Allocate space or copy an array, then read results when the work completes.

[Back to overview](gpu.md)

## Buffers

`GpuBuffer<T>.alloc(n)`, `.from(data)`, `.vertex_from(data)`. Then `.length`, `write` / `write_at`, `read.await` / `read_at`, `copy_to`. `GpuSwap<T>` is a front/back pair (`swap()`).

## Create and update a buffer

`GpuBuffer<T>.alloc(count)` allocates room for elements; `GpuBuffer<T>.from(array)` uploads existing data. For vertex data, use `vertex(count)` or `vertex_from(array)`.

The following setup snippet belongs inside an async function after successful GPU initialization:

```dream
let values = GpuBuffer<float>.from([1.0f, 2.0f, 3.0f]);
let copy = values.read().await;
System.println(copy[0]);
```

`write(array)` updates from the start; `write_at(offset, array)` starts at an element position. `read()` retrieves all elements, and `read_at(offset, count)` retrieves a range. Keep ranges within `.length`. Read `.stride` for the stored byte size of one element.

`copy_to` copies a range between buffers. `destroy()` releases the GPU resource explicitly; do not use it afterward. See [all buffer signatures](../api/gpu-gpu-buffer.md).

# GPU: SAXPY

Run a tiny **compute** shader: `out[i] = 2 * x[i] + y[i]`, then print the result. Needs a GPU (or `gpu unavailable`).

```dream
import system;
import system.gpu;

@compute(64)
fun saxpy(x: GpuBuffer<float>, y: GpuBuffer<float>, out: GpuBuffer<float>, n: int): void {
    let i = global_id.x;
    if i < n {
        out[i] = 2.0 * x[i] + y[i];
    }
}

async fun main(): void {
    let init = await Gpu.try_init();
    if init.is_err() {
        System.println("gpu unavailable");
        return;
    }

    let x = GpuBuffer.from([1.0, 2.0, 3.0, 4.0]);
    let y = GpuBuffer.from([10.0, 20.0, 30.0, 40.0]);
    let out = GpuBuffer<float>.alloc(4);
    let r = await Compute.run_1d("saxpy", [x, y, out], 4);
    if r.is_err() {
        System.println("dispatch failed");
        return;
    }

    let vals = await out.read();
    System.println((int)vals[0]);   // 12
    System.println((int)vals[1]);   // 24
    System.println((int)vals[2]);   // 36
    System.println((int)vals[3]);   // 48
}
```

```bash
dream run saxpy.dream
# in this repo:
dream run sample/compute/saxpy.dream
```

- `@compute(64)` — workgroup size 64; the body becomes WGSL.
- `global_id.x` — this thread’s index.
- `Compute.run_1d("saxpy", buffers, 4)` — run 4 threads; the name must match the function.

Draw a triangle instead: [GPU triangle](gpu-triangle.md). API: [`system.gpu`](../reference/stdlib/gpu.md), [compute shaders](../reference/language/compute.md).

# Compute shaders (`@compute`)

A compute function runs calculations on a GPU. Mark it with `@compute`, supply buffers through `system.gpu`, and choose how much work to run. Start small before adding several passes.

You can start with a one-kernel SAXPY, build a reaction–diffusion sim, or study the full fluid demo.
Native `dream run` and the browser both run WGSL via wgpu / WebGPU when a GPU adapter is available (see [stdlib GPU](../stdlib/gpu.md)).
Headless environments without an adapter print `gpu unavailable` from `Gpu.try_init`.

## Explore this topic

- [Compute examples](compute-examples.md)
- [Write a compute function](compute-rules.md)
- [Run several compute passes](compute-passes.md)

## Quick start

```dream
import system;
import system.gpu;

@compute(64)
fun add(a: GpuBuffer<float>, b: GpuBuffer<float>, out: GpuBuffer<float>, n: int): void {
    let i = global_id.x;
    if i < a.length && i < n {
        out[i] = a[i] + b[i];
    }
}

async fun main(): void {
    let init = Gpu.try_init().await;
    if init.is_err() { return; }
    let a = GpuBuffer.from([1.0, 2.0, 3.0]);
    let b = GpuBuffer.from([10.0, 20.0, 30.0]);
    let out = GpuBuffer<float>.alloc(3);
    let r = Compute.run_1d("add", [a, b, out], 3).await;
    if r.is_err() { return; }
    let vals = out.read().await;
    System.println((int)vals[0]); // 11 when a GPU adapter is available
}
```

## See also

- [stdlib GPU](../stdlib/gpu.md)
- Beginner: [`saxpy.dream`](https://github.com/sps014/dream/tree/main/sample/compute/saxpy.dream)
- Complex: [`sample/compute/life/`](https://github.com/sps014/dream/tree/main/sample/compute/life)
- Fluid: [`sample/fluid/`](https://github.com/sps014/dream/tree/main/sample/fluid)

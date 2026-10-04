# Compute examples

Start with the smallest example. Larger simulations combine several passes and need more setup.

[Back to overview](compute.md)

## Simple sample: SAXPY

[`sample/compute/saxpy.dream`](https://github.com/sps014/dream/tree/main/sample/compute/saxpy.dream)
— one kernel, one dispatch, readback:

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
    let init = Gpu.try_init().await;
    if init.is_err() { return; }
    let x = GpuBuffer.from([1.0, 2.0, 3.0, 4.0]);
    let y = GpuBuffer.from([10.0, 20.0, 30.0, 40.0]);
    let out = GpuBuffer<float>.alloc(4);
    let _ = Compute.run_1d("saxpy", [x, y, out], 4).await;
    let vals = out.read().await;
    System.println((int)vals[0]); // browser: 12
}
```

```bash
dream run sample/compute/saxpy.dream
```


## Complex sample: reaction–diffusion

[`sample/compute/life/`](https://github.com/sps014/dream/tree/main/sample/compute/life) —
Gray–Scott chemical simulation (not cellular automata): dual fields, multi-step
`ComputePass`, texture paint.

```dream
@compute(8, 8)
fun rd_step(
    @readonly u_in: GpuBuffer<float>,
    @readonly v_in: GpuBuffer<float>,
    u_out: GpuBuffer<float>,
    v_out: GpuBuffer<float>,
    n: int, _ey: int, _ez: int,
    du: float, dv: float, feed: float, kill: float
): void {
    // … laplacian(u/v) + u·v² reaction …
}

@compute(8, 8)
fun rd_paint(
    @readonly u: GpuBuffer<float>,
    @readonly v: GpuBuffer<float>,
    @storage tex: GpuTexture,
    n: int
): void {
    Gpu.texture_store(tex, global_id.x, global_id.y, /* palette from v */);
}
```

Host batch (browser path):

```dream
let pass = ComputePass.begin();
// several rd_step dispatches (ping-pong), then:
pass.dispatch_resources(
    "rd_paint",
    [u.id, v.id],
    [tex.id],
    Buffer.alloc<int>(0),
    n, n, 1,
    Buffer.alloc<byte>(0)
);
let _ = pass.submit().await;
GpuRenderPass.blit(surface, tex).await;
```

```bash
dream run sample/compute/life/life.dream
# serve the folder → sample/compute/life/life.html
```


## Larger demo: fluid

[`sample/fluid/`](https://github.com/sps014/dream/tree/main/sample/fluid) — Jos Stam–style
2D stable fluids on the GPU: `ComputePass` batches splat / advect / Jacobi project / decay /
paint; the CPU tracks mouse via `GpuSurface.pointer()` / `poll_events()` (works in the browser and
under `dream run` — no `js.global` DOM listeners).

```dream
@compute(8, 8)
fun advect(
    @readonly src: GpuBuffer<float>,
    dst: GpuBuffer<float>,
    @readonly vx: GpuBuffer<float>,
    @readonly vy: GpuBuffer<float>,
    n: int
): void {
    let x = global_id.x;
    let y = global_id.y;
    if x >= n || y >= n { return; }
    // … bilinear sample of src at (x - dt*vx, y - dt*vy) …
}

// Host: several ComputePass submits per frame, then blit.
let pass = ComputePass.begin();
pass.dispatch("advect", [vx, vx_tmp, vx, vy], n, n, 1);
pass.dispatch("advect", [vy, vy_tmp, vx, vy], n, n, 1);
let _ = pass.submit().await;
```

```bash
dream run sample/fluid/fluid.dream
# or serve the folder → sample/fluid/fluid.html
```


## Samples

| Sample | Role |
|--------|------|
| [`sample/compute/saxpy.dream`](https://github.com/sps014/dream/tree/main/sample/compute/saxpy.dream) | Beginner — one kernel + readback |
| [`sample/compute/gpu_ext.dream`](https://github.com/sps014/dream/tree/main/sample/compute/gpu_ext.dream) | API surface — `@readonly`, `ComputePass`, indirect |
| [`sample/compute/life/`](https://github.com/sps014/dream/tree/main/sample/compute/life) | Complex — Gray–Scott reaction–diffusion |
| [`sample/fluid/`](https://github.com/sps014/dream/tree/main/sample/fluid) | Larger demo — interactive stable fluids |

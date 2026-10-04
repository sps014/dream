# Run several compute passes

Use separate passes when one calculation needs the result of another.

[Back to overview](compute.md)

## Multi-pass sync

WebGPU has **no** global barrier across workgroups.
Algorithms that need one (e.g. Jacobi pressure solve) issue multiple dispatches; host queue order provides happens-before.

Prefer **`ComputePass`** to batch several dispatches into one submit:

```dream
let pass = ComputePass.begin();
pass.dispatch("advect", [src, dst, vx, vy], n, n, 1);
pass.dispatch("divergence", [vx, vy, div], n, n, 1);
let _ = pass.submit().await;
```

For GPU-written workgroup counts, pack three i32s with `GpuDispatchIndirect` and call `Compute.dispatch_indirect` (or `pass.dispatch_indirect`).


## Escape hatch

```dream
let shader = GpuShader.from_wgsl(WGSL_SOURCE, "main");
let r = Compute.run_shader(shader, [buf], 64, 1, 1).await;
```

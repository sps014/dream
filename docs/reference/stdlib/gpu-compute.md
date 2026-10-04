# Run GPU calculations

Submit a compute function with its inputs and work size. Use a pass when several calculations belong together.

[Back to overview](gpu.md)

## Dispatch (`@compute`)

`Compute.run_1d(name, buffers, count)`, `run_2d` / `run_3d`, `run_2d_uniforms`, `run_resources`, `dispatch_indirect`, `run_shader`. Bind with `GpuBindList`. Pack CPU values with `Uniforms.pack`. `ComputePass` batches several dispatches then `submit()`.

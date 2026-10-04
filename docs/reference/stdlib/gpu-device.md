# Set up a GPU device

Check that a device is available and inspect its supported features before creating resources.

[Back to overview](gpu.md)

## Device and time

| Call | Meaning |
| --- | --- |
| `Gpu.is_available` | adapter present? |
| `Gpu.try_init().await` | request device (high-performance adapter) |
| `Gpu.try_init(GpuPowerPreference.LowPower).await` | same, preferring battery-friendly GPUs |
| `Gpu.ready` | init succeeded (false after device-lost until `try_init` again) |
| `Gpu.check()` | pending uncaptured error or device-lost, else `Ok` |
| `Gpu.frame().await` | wait a display frame |
| `Gpu.timestamp().await` | host monotonic clock (CPU), not GPU |
| `Gpu.timestamp_period()` | nanoseconds per GPU timestamp tick |
| `Gpu.capabilities()` | optional features and limits the device got |


## Capabilities

- WebGPU only lets a shader or resource touch a feature the *device* opted into when it was created.
- Reaching for an un-requested one is device-loss-grade rather than a recoverable validation error.
- Dream requests every optional feature the adapter offers, so `Gpu.capabilities()` is the authoritative answer to "may I use this?" — gate on it rather than trying and recovering.
- Everything reads as `false` / `0` until `try_init` succeeds, since capabilities describe the negotiated device rather than the raw adapter.

```dream
let caps = Gpu.capabilities();
if caps.texture_compression_astc {
    // mobile-sized asset path
}
let tile = caps.max_invocations_per_workgroup;
```

| Field | Meaning |
| --- | --- |
| `shader_float16` | half-precision arithmetic in shaders |
| `subgroup`, `subgroup_barrier` | subgroup operations; the barrier is native-only |
| `min_subgroup_size`, `max_subgroup_size` | subgroup width range, `0` when unreported |
| `tile_float16`, `tile_float`, `tile_n` | cooperative-matrix tiles; reserved, always off today |
| `texture_compression_bc` / `_etc2` / `_astc` | compressed format families (BC desktop, ETC2/ASTC mobile) |
| `float32_filterable` | linear filtering of 32-bit float textures |
| `timestamp_query` | GPU timestamp query sets (`ComputePass.begin_timed`, `GpuRenderTarget.timestamps`) |
| `timestamp_query_inside_encoders` | `GpuEncoder.write_timestamp` between passes |
| `timestamp_query_inside_passes` | `write_timestamp` inside an open compute or render pass |
| `max_buffer_bytes`, `max_storage_binding_bytes` | allocation and storage-binding ceilings |
| `max_workgroup_storage_bytes` | `var<workgroup>` bytes per workgroup |
| `max_invocations_per_workgroup`, `max_workgroup_size_x/y/z` | `@workgroup_size` ceilings |
| `max_workgroups_per_dimension` | per-dimension dispatch ceiling |

- Buffer sizes and the compute workgroup limits are raised to the adapter maximum.
- Every other limit stays at the portable WebGPU default, so a program developed against a large GPU still runs on a small one.
- Creating a `Bc*` / `Etc2*` / `Astc*` texture without the matching flag fails with `GpuError.unsupported`.

`GpuError` implements [`Error`](option-result.md). Headless machines often have no adapter. Async GPU methods take an optional last `token`; cancelled `Result` calls return `GpuError` `ECANCELLED`. A lost device (`DEVICE_LOST`) is distinct from `VALIDATION`: recover with `Gpu.try_init().await` and recreate GPU resources. `Gpu.check()` drains a pending lost / uncaptured event without waiting for the next submit.

- `Gpu.try_init(GpuPowerPreference.LowPower)` (or `Default`) is only consulted when no device is alive yet; after a loss, a different preference re-picks the adapter.
- `try_init()` keeps high-performance.

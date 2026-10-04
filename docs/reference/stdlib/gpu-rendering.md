# Textures and rendering

Create resources, bind them to your rendering functions, and draw to a surface.

[Back to overview](gpu.md)

## Textures, surfaces, draw

`GpuTexture.rgba8` (and depth / float / cube variants), `GpuTexture.from_image_bytes(png_or_jpeg).await` for PNG/JPEG decode, `GpuSampler.linear()` / `nearest()`. `GpuSurface.create` / `from_canvas`, `configure(w, h)` or `configure(GpuSurfaceDesc)` (`present_mode`, `alpha_mode`, `color_space`, `max_pixel_ratio`), `present()`, input helpers (`pointer()`, `pointers()`, pointer lock, fullscreen, gamepad axes), `GpuRenderPass.draw` / `blit`. Vertex path: `GpuRenderPipeline.create_ex`, `GpuVec2` / `GpuVec4`, `@builtin("position")`. GPU pass timing: `GpuQuerySet.timestamps(n)` then `ComputePass.begin_timed(qs, 0, 1)`, `GpuRenderTarget.timestamps(qs, 0, 1)`, `GpuEncoder.write_timestamp(qs, i)` (`timestamp_query_inside_encoders`), or `pass.write_timestamp(qs, i)` (`timestamp_query_inside_passes`); `qs.read()` after submit.

Shader-only (calling them from CPU code is a compile error): `Gpu.workgroup_barrier` / `storage_barrier`, `Gpu.atomic_*` (`atomic_load`, `atomic_store`, `atomic_add`, `atomic_sub`, `atomic_min`, `atomic_max`, `atomic_and`, `atomic_or`, `atomic_xor`, `atomic_exchange`, `atomic_compare_exchange`), `Gpu.dpdx` / `dpdy` / `fwidth` (derivatives), and every `Gpu.texture_*` read or write except `texture_dimensions` (`texture_load*`, `texture_store`, `texture_sample*`, `texture_gather`, `texture_num_levels` / `texture_num_layers`).

`GpuMath` works in shaders and on the CPU, where it computes the same result with host `Math`.

## Choose texture settings

`GpuTextureDesc` describes dimensions, format, layers, mip levels, sample count, and optional storage access. Use its named factories such as `d2`, `d2_array`, `d3`, `cube`, and `cube_array` rather than leaving settings unspecified.

The following snippet requires a successfully initialized GPU:

```dream
let description = GpuTextureDesc.d2(256, 256, GpuTextureFormat.Rgba8Unorm);
switch (GpuTexture.create(description)) {
    Ok(texture) => System.println(texture.width),
    Err(error) => System.println(error.message()),
}
```

Creation can fail for an unsupported format or combination of settings. Check the [device capabilities](gpu-device.md) before relying on optional features. See [texture settings](../api/gpu-gpu-texture-desc.md), [texture methods](../api/gpu-gpu-texture.md), and [format choices](../api/gpu-gpu-enums.md).

## Choose how a texture is sampled

`GpuSamplerDesc` controls filtering and what happens outside a texture's edges. Start with `linear()` for smooth filtering or `nearest()` for exact neighboring pixels. `GpuFilterMode` selects nearest or linear filtering. `GpuAddressMode` selects edge clamping, repeating, or mirrored repeating.

Use `mag_filter`, `min_filter`, and `mip_filter` for filtering at different scales. `address_u`, `address_v`, and `address_w` control the axes. `lod_min` and `lod_max` limit mip levels. `compare` enables a comparison sampler for depth tests; it is not interchangeable with an ordinary color sampler. Anisotropy above one requires linear filtering for all three filter settings.

See [sampler settings](../api/gpu-gpu-sampler-desc.md) and [sampler methods](../api/gpu-gpu-sampler.md).

## Record drawing work

`GpuRenderPassEncoder` records a pass: select its pipeline, bind resources, set vertex or index buffers, issue draws, and end the pass. `GpuBindGroup` groups resources for binding. Settings such as `GpuStoreOp`, `GpuIndexFormat`, and vertex format describe how the pass stores results and reads its inputs.

Read [rendering data](../language/shader-resources.md) before choosing these settings. Exact declarations are in the [render encoder](../api/gpu-gpu-encoder.md), [binding group](../api/gpu-gpu-bind-group.md), and [render pipeline settings](../api/gpu-gpu-render-pipeline-desc.md) references.

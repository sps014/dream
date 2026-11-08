---
title: Ray tracing
summary: Build acceleration structures and trace rays with ray queries or ray tracing pipelines.
icon: material/flare
api: reference/api/system-gpu.md
---

# Ray tracing

Ray tracing finds where rays hit triangles. Build an acceleration structure once, then search it from a shader:

- Ray queries trace from inside any shader, usually a compute shader, and give you the hit to
  handle in place. They are the simpler choice for shadows, visibility and picking.
- Ray tracing pipelines run separate shaders for the camera, for hits and for misses. They suit
  full renderers with many materials.

Neither runs on WebGPU.

- Ray queries need `GpuFeature.RayQuery` (Metal, Vulkan, and Direct3D 12).
- Ray tracing pipelines need `GpuFeature.RayTracing` (Vulkan and Direct3D 12).

For a query in a fragment shader, declare a `GpuAccelerationStructure` parameter and bind the scene with `pass.bind(GpuShaders.<shader>.scene, scene)` before drawing.

Check `device.supports(GpuFeature.RayQuery)` first and compile for a native target. The `GpuRayQuery` calls below work in a fragment shader; WebGPU cannot contain them.

## Building a scene

A bottom-level structure holds triangles: three `GpuVec3` corners per triangle in a `GpuBuffer`. A top-level structure places those with a transform each:

```dream
import system;
import system.gpu;

async fun scene(device: GpuDevice): Result<GpuAccelerationStructure, GpuError> {
    let corners = device.buffer_from(
        [GpuVec3(-0.8f, -0.8f, 0.0f), GpuVec3(0.8f, -0.8f, 0.0f), GpuVec3(0.0f, 0.8f, 0.0f)]
    )?;
    let triangle = device.build_bottom_level(GpuGeometry.triangles(corners)).await?;

    let near = GpuInstance(triangle, GpuMat4.identity()).with_id(1);
    let far = GpuInstance(triangle, GpuMat4.translation(GpuVec3(0.0f, 0.0f, 5.0f))).with_id(2);
    return device.build_top_level([near, far]).await;
}
```

The same bottom level can appear many times with different transforms. `with_id` sets the number
that a hit reports as `hit_instance_id()`, so a shader can tell instances apart.

## Ray queries

This program traces one ray per cell of a 16 by 16 grid, straight into the scene, and prints `#`
where a ray hits the triangle:

```dream
import system;
import system.gpu;

@gpu_targets(GpuTarget.Metal, GpuTarget.Vulkan)
@compute(8, 8, 1)
fun shadow_mask(scene: GpuAccelerationStructure, hits: GpuBuffer<int>, size: int): void {
    let cell = global_id();
    if cell.x >= size || cell.y >= size {
        return;
    }

    let u = ((float)cell.x + 0.5f) / (float)size * 2.0f - 1.0f;
    let v = 1.0f - ((float)cell.y + 0.5f) / (float)size * 2.0f;
    let ray = GpuRay(GpuVec3(u, v, -2.0f), GpuVec3(0.0f, 0.0f, 1.0f), 0.0f, 100.0f);

    let query = GpuRayQuery(GpuRayFlags.ForceOpaque);
    query.trace(scene, 255u, ray);
    while query.proceed() {
        query.commit_triangle();
    }

    if query.has_hit() {
        hits[cell.y * size + cell.x] = 1;
    } else {
        hits[cell.y * size + cell.x] = 0;
    }
}

async fun run(): Result<Unit, GpuError> {
    let device = GpuDevice.create(GpuBackend.Auto).await?;
    if !device.supports(GpuFeature.RayQuery) {
        System.println(device.backend.to_string() + " has no ray queries");
        return Result.Ok(Unit());
    }

    let corners = device.buffer_from(
        [GpuVec3(-0.8f, -0.8f, 0.0f), GpuVec3(0.8f, -0.8f, 0.0f), GpuVec3(0.0f, 0.8f, 0.0f)]
    )?;
    let triangle = device.build_bottom_level(GpuGeometry.triangles(corners)).await?;
    let scene = device.build_top_level([GpuInstance(triangle, GpuMat4.identity())]).await?;

    let size = 16;
    let hits = device.buffer_alloc<int>(size * size)?;
    GpuShaders.shadow_mask.dispatch(device, GpuDispatch.threads_2d(size, size), scene, hits, size)
        .await?;

    let mask = hits.read().await?;
    for (let y = 0; y < size; y = y + 1) {
        let row = StringBuilder();
        for (let x = 0; x < size; x = x + 1) {
            if mask[y * size + x] == 1 {
                row.append("#");
            } else {
                row.append(".");
            }
        }
        System.println(row.to_string());
    }
    return Result.Ok(Unit());
}

async fun main(): void {
    switch (run().await) {
        Ok(_) => {}
        Err(e) => {
            System.println("GPU error: " + e.message());
        }
    }
}
```

On a GPU with ray queries it prints a triangle of `#` characters, widest near the bottom.

A ray starts at `origin`, travels along `direction`, and counts hits only between `t_min` and `t_max`. Opaque hits are accepted on their own.

- `proceed` stops at each non-opaque candidate. `commit_triangle` accepts it; skip that call to ignore a hit, for example where a texture is transparent.
- With `ForceOpaque`, every triangle is opaque, so the loop body never runs. The loop is still required to finish the trace.
- After that, `has_hit`, `hit_distance`, `hit_instance_id`, and the other getters describe the closest accepted hit.

`ForceOpaque` treats every triangle as opaque, and `AcceptFirstHitAndEndSearch` stops at the first hit, which is enough for a shadow. The `255u` mask hides instances that share no bits with it.

## Ray tracing pipelines

A ray tracing pipeline splits the work across shaders:

| Stage | Runs |
|---|---|
| `@ray_generation` | once per pixel; it calls `trace_ray` |
| `@closest_hit` | for the nearest hit of a ray |
| `@any_hit` | for every candidate hit, to accept or ignore it |
| `@miss` | when a ray hits nothing |
| `@intersection` | to test custom shapes built with `GpuGeometry.boxes` |
| `@callable` | when another stage calls `call_shader` |

The stages share a payload struct. Mark its fields with `@ray_payload`, take it by `ref`, and write the result there:

```dream
import system;
import system.gpu;

public struct Shade {
    @ray_payload
    public color: GpuVec3;
}

@gpu_targets(GpuTarget.Vulkan)
@ray_generation
fun camera(scene: GpuAccelerationStructure, image: GpuStorageTexture<GpuVec4>): void {
    let pixel = dispatch_ray_index().xy;
    let size = dispatch_ray_dimensions().xy;
    let u = ((float)pixel.x + 0.5f) / (float)size.x * 2.0f - 1.0f;
    let v = 1.0f - ((float)pixel.y + 0.5f) / (float)size.y * 2.0f;
    let ray = GpuRay(GpuVec3(u, v, -2.0f), GpuVec3(0.0f, 0.0f, 1.0f), 0.001f, 100.0f);

    let shade = Shade();
    trace_ray(scene, GpuRayFlags.None, 255u, ray, ref shade);
    image[pixel] = GpuVec4(shade.color.x, shade.color.y, shade.color.z, 1.0f);
}

@gpu_targets(GpuTarget.Vulkan)
@closest_hit
fun surface(ref shade: Shade, hit: GpuTriangleHit): void {
    shade.color = GpuVec3(1.0f, hit.barycentrics.x, hit.barycentrics.y);
}

@gpu_targets(GpuTarget.Vulkan)
@miss
fun sky(ref shade: Shade): void {
    shade.color = GpuVec3(0.0f, 0.0f, 1.0f);
}

async fun render(
    device: GpuDevice,
    scene: GpuAccelerationStructure,
    size: int
): Result<GpuVec4[], GpuError> {
    let image = device.storage_texture<GpuVec4>(size, size)?;
    let pipeline = device.ray_tracing_pipeline(
        GpuShaders.camera,
        [GpuShaders.sky],
        [GpuHitGroup.closest(GpuShaders.surface)],
        []
    )?;
    GpuShaders.camera.trace(device, pipeline, size, size, scene, image).await?;
    return image.read().await;
}
```

`ray_tracing_pipeline` takes the ray generation shader, the miss shaders, the hit groups, and the callable shaders. A hit group always has a closest-hit shader; add `with_any_hit` or `with_intersection` for the others.

The generated `trace` method runs the ray generation shader once per pixel of a `width` by `height` grid. Its remaining arguments are that shader's parameters.

Only the ray generation shader takes resource parameters. The payload is at most 128 bytes, hit attributes at most 32, and a hit shader cannot trace further rays.

use crate::error::{self, HostResult};
use crate::{devices, pipelines, raytracing, resources, submit, surfaces};
use dream_host_abi::{DreamPtr, alloc_i32s, alloc_string, read_bytes, read_i32s, read_string};

fn handle(result: HostResult<i32>) -> i32 {
    result.unwrap_or_else(|e| {
        error::record(&e);
        0
    })
}

fn status(result: HostResult<()>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(e) => {
            error::record(&e);
            1
        }
    }
}

fn future(pointer: DreamPtr) -> usize {
    pointer as usize
}

#[unsafe(no_mangle)]
pub extern "C" fn gpuDeviceCreateAsync(future_ptr: DreamPtr, backend: i32, compiled: i32) -> i32 {
    devices::create(future(future_ptr), backend, compiled);
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn gpuLastError() -> DreamPtr {
    alloc_string(&error::last_text())
}

#[unsafe(no_mangle)]
pub extern "C" fn gpuDeviceBackend(device: i32) -> i32 {
    handle(devices::backend(device))
}

#[unsafe(no_mangle)]
pub extern "C" fn gpuDeviceFeatures(device: i32) -> i32 {
    devices::features(device).unwrap_or_else(|e| {
        error::record(&e);
        0
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn gpuDeviceCheck(device: i32) -> i32 {
    status(devices::check(device))
}

/// # Safety
/// `bundle` must be a readable guest byte array for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gpuLibraryLoad(device: i32, bundle: DreamPtr) -> i32 {
    let bytes = unsafe { read_bytes(bundle) };
    handle(pipelines::library_load(device, &bytes))
}

#[unsafe(no_mangle)]
pub extern "C" fn gpuBufferCreate(device: i32, byte_size: i32, usage: i32) -> i32 {
    handle(resources::buffer_create(device, byte_size, usage))
}

/// # Safety
/// `data` must be a readable guest byte array for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gpuBufferWrite(buffer: i32, byte_offset: i32, data: DreamPtr) -> i32 {
    let bytes = unsafe { read_bytes(data) };
    status(resources::buffer_write(buffer, byte_offset, &bytes))
}

#[unsafe(no_mangle)]
pub extern "C" fn gpuBufferReadAsync(future_ptr: DreamPtr, buffer: i32, byte_offset: i32, byte_size: i32) -> i32 {
    resources::buffer_read(future(future_ptr), buffer, byte_offset, byte_size);
    1
}

/// # Safety
/// `desc` must be a readable guest int array for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gpuTextureCreate(device: i32, desc: DreamPtr) -> i32 {
    let desc = unsafe { read_i32s(desc) };
    handle(resources::texture_create(device, &desc))
}

/// # Safety
/// `data` must be a readable guest byte array for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gpuTextureWrite(texture: i32, mip: i32, layer: i32, data: DreamPtr) -> i32 {
    let bytes = unsafe { read_bytes(data) };
    status(resources::texture_write(texture, mip, layer, &bytes))
}

#[unsafe(no_mangle)]
pub extern "C" fn gpuTextureReadAsync(future_ptr: DreamPtr, texture: i32, mip: i32, layer: i32) -> i32 {
    resources::texture_read(future(future_ptr), texture, mip, layer);
    1
}

/// # Safety
/// `desc` must be a readable guest int array for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gpuSamplerCreate(device: i32, desc: DreamPtr) -> i32 {
    let desc = unsafe { read_i32s(desc) };
    handle(resources::sampler_create(device, &desc))
}

#[unsafe(no_mangle)]
pub extern "C" fn gpuDescriptorHandle(resource: i32) -> u64 {
    resources::descriptor_handle(resource).unwrap_or_else(|e| {
        error::record(&e);
        0
    })
}

/// # Safety
/// `entry` must be a readable guest string for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gpuComputePipelineCreate(library: i32, entry: DreamPtr) -> i32 {
    let entry = unsafe { read_string(entry) };
    handle(pipelines::compute_pipeline_create(library, &entry))
}

/// # Safety
/// The entry names and `desc` must be readable guest payloads for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gpuRenderPipelineCreate(
    vertex_library: i32,
    vertex_entry: DreamPtr,
    fragment_library: i32,
    fragment_entry: DreamPtr,
    desc: DreamPtr,
) -> i32 {
    let (vertex, fragment, desc) = unsafe {
        (
            read_string(vertex_entry),
            read_string(fragment_entry),
            read_i32s(desc),
        )
    };
    handle(pipelines::render_pipeline_create(
        vertex_library,
        &vertex,
        fragment_library,
        &fragment,
        &desc,
    ))
}

/// # Safety
/// `libraries`, `entries` and `layout` must be readable guest payloads for the duration of
/// this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gpuRayTracingPipelineCreate(
    device: i32,
    libraries: DreamPtr,
    entries: DreamPtr,
    layout: DreamPtr,
) -> i32 {
    let (libraries, entries, layout) = unsafe { (read_i32s(libraries), read_string(entries), read_i32s(layout)) };
    handle(raytracing::ray_tracing_pipeline_create(device, &libraries, &entries, &layout))
}

/// # Safety
/// `geometry` must be a readable guest int array for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gpuBottomLevelBuildAsync(future_ptr: DreamPtr, device: i32, geometry: DreamPtr) -> i32 {
    let geometry = unsafe { read_i32s(geometry) };
    raytracing::bottom_level_build(future(future_ptr), device, &geometry);
    1
}

/// # Safety
/// `instances` and `transforms` must be readable guest arrays for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gpuTopLevelBuildAsync(
    future_ptr: DreamPtr,
    device: i32,
    instances: DreamPtr,
    transforms: DreamPtr,
) -> i32 {
    // A guest float array has the int array's layout; the elements are reinterpreted.
    let (instances, transforms) = unsafe { (read_i32s(instances), read_i32s(transforms)) };
    let transforms: Vec<f32> = transforms.into_iter().map(|bits| f32::from_bits(bits as u32)).collect();
    raytracing::top_level_build(future(future_ptr), device, &instances, &transforms);
    1
}

/// # Safety
/// `weights` and `layout` must be readable guest arrays for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gpuMlpUpload(device: i32, weights: DreamPtr, layout: DreamPtr) -> i32 {
    let (weights, layout) = unsafe { (read_bytes(weights), read_i32s(layout)) };
    handle(resources::mlp_upload(device, &weights, &layout))
}

#[unsafe(no_mangle)]
pub extern "C" fn gpuSurfaceCreate(device: i32, window: i32, vsync: i32) -> i32 {
    handle(surfaces::surface_create(device, window, vsync != 0))
}

#[unsafe(no_mangle)]
pub extern "C" fn gpuSurfaceResize(surface: i32, width: i32, height: i32) -> i32 {
    status(surfaces::surface_resize(surface, width, height))
}

#[unsafe(no_mangle)]
pub extern "C" fn gpuSurfaceInfo(surface: i32) -> DreamPtr {
    match surfaces::surface_info(surface) {
        Ok(info) => alloc_i32s(&info),
        Err(e) => {
            error::record(&e);
            alloc_i32s(&[])
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn gpuSurfaceAcquireAsync(future_ptr: DreamPtr, surface: i32) -> i32 {
    surfaces::acquire(future(future_ptr), surface);
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn gpuSurfacePresentAsync(future_ptr: DreamPtr, surface: i32) -> i32 {
    surfaces::present(future(future_ptr), surface);
    1
}

/// # Safety
/// `commands` and `data` must be readable guest arrays for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gpuSubmitAsync(future_ptr: DreamPtr, device: i32, commands: DreamPtr, data: DreamPtr) -> i32 {
    let (words, bytes) = unsafe { (read_i32s(commands), read_bytes(data)) };
    submit::submit(future(future_ptr), device, &words, &bytes);
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn gpuRelease(handle: i32) {
    crate::state::lock().release(handle);
}

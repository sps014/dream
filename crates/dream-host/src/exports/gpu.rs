//! GPU guest C ABI.

use super::abi::*;
use crate::gpu::{
    attach_abi_from_wat_path, buffers, caps, compute, device, error, render, surface, textures,
};
use std::sync::Once;

fn ensure_abi() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        if let Ok(p) = std::env::var("DREAM_NATIVE_MODULE") {
            attach_abi_from_wat_path(&p);
        }
    });
}

#[no_mangle]
pub extern "C" fn gpuIsAvailable() -> i32 {
    i32::from(device::is_available())
}

#[no_mangle]
pub extern "C" fn gpuCapabilities() -> usize {
    alloc_bytes(&caps::encode())
}

#[no_mangle]
pub extern "C" fn gpuReady() -> i32 {
    i32::from(crate::gpu::is_ready())
}

#[no_mangle]
pub extern "C" fn gpuLastError() -> usize {
    alloc_string(&error::take_last_error())
}

#[no_mangle]
pub extern "C" fn gpuCheck() -> i32 {
    error::poll_status()
}

#[no_mangle]
pub extern "C" fn gpuTryInit(power: i32) -> i32 {
    ensure_abi();
    device::try_init(power)
}

#[no_mangle]
pub extern "C" fn gpuFrame() -> i32 {
    surface::wait_display_frame();
    crate::gpu::profile::end_frame();
    0
}

#[no_mangle]
pub extern "C" fn gpuTimestamp() -> i64 {
    use std::time::Instant;
    static ORIGIN: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    ORIGIN.get_or_init(Instant::now).elapsed().as_nanos() as i64
}

#[no_mangle]
pub extern "C" fn gpuBufferAllocBytes(n: i32) -> i32 {
    buffers::alloc_bytes(n)
}

#[no_mangle]
pub extern "C" fn gpuBufferAllocVertexBytes(n: i32) -> i32 {
    buffers::alloc_vertex_bytes(n)
}

#[no_mangle]
pub unsafe extern "C" fn gpuBufferWriteBytes(id: i32, data: usize) {
    let _ = buffers::write_bytes(id, read_bytes(data));
}

#[no_mangle]
pub unsafe extern "C" fn gpuBufferWriteBytesAt(id: i32, off: i32, data: usize) {
    let _ = buffers::write_bytes_at(id, off, read_bytes(data));
}

#[no_mangle]
pub extern "C" fn gpuBufferReadBytes(id: i32, n: i32) -> usize {
    match buffers::read_bytes(id, n) {
        Ok(b) => alloc_bytes(&b),
        Err(_) => 0,
    }
}

#[no_mangle]
pub extern "C" fn gpuBufferReadBytesAt(id: i32, off: i32, n: i32) -> usize {
    match buffers::read_bytes_at(id, off, n) {
        Ok(b) => alloc_bytes(&b),
        Err(_) => 0,
    }
}

#[no_mangle]
pub extern "C" fn gpuBufferCopy(src: i32, dst: i32, src_off: i32, dst_off: i32, size: i32) {
    buffers::copy(src, dst, src_off, dst_off, size);
}

#[no_mangle]
pub extern "C" fn gpuBufferDestroy(id: i32) {
    buffers::destroy(id);
}

#[no_mangle]
pub unsafe extern "C" fn gpuDispatch(
    kernel: usize,
    bufs: usize,
    tex: usize,
    samp: usize,
    ex: i32,
    ey: i32,
    ez: i32,
    uniforms: usize,
) -> i32 {
    compute::dispatch(
        &read_string(kernel),
        &read_i32s(bufs),
        &read_i32s(tex),
        &read_i32s(samp),
        ex,
        ey,
        ez,
        &read_bytes(uniforms),
    )
}

#[no_mangle]
pub unsafe extern "C" fn gpuDispatchIndirect(
    kernel: usize,
    bufs: usize,
    tex: usize,
    samp: usize,
    indirect: i32,
    off: i32,
) -> i32 {
    compute::dispatch_indirect(
        &read_string(kernel),
        &read_i32s(bufs),
        &read_i32s(tex),
        &read_i32s(samp),
        indirect,
        off,
    )
}

#[no_mangle]
pub unsafe extern "C" fn gpuSurfaceCreate(title: usize, w: i32, h: i32) -> i32 {
    surface::create(&read_string(title), w, h)
}

#[no_mangle]
pub extern "C" fn gpuSurfaceConfigure(
    id: i32,
    w: i32,
    h: i32,
    present_mode: i32,
    alpha_mode: i32,
    color_space: i32,
    max_pixel_ratio: f32,
) {
    surface::configure(
        id,
        w,
        h,
        present_mode,
        alpha_mode,
        color_space,
        max_pixel_ratio,
    );
}

#[no_mangle]
pub extern "C" fn gpuSurfacePresent(id: i32) -> i32 {
    surface::present(id)
}

#[no_mangle]
pub extern "C" fn gpuSurfaceCloseRequested(id: i32) -> i32 {
    i32::from(surface::close_requested(id))
}

#[no_mangle]
pub extern "C" fn gpuSurfaceWidth(id: i32) -> i32 {
    surface::width(id)
}

#[no_mangle]
pub extern "C" fn gpuSurfaceHeight(id: i32) -> i32 {
    surface::height(id)
}

#[no_mangle]
pub extern "C" fn gpuSurfaceFocused(id: i32) -> i32 {
    i32::from(surface::focused(id))
}

#[no_mangle]
pub extern "C" fn gpuSurfaceDestroy(id: i32) {
    surface::destroy(id);
}

#[no_mangle]
pub unsafe extern "C" fn gpuRenderPipelineCreate(vs: usize, fs: usize) -> i32 {
    render::pipeline_create_ex(&read_string(vs), &read_string(fs), 0, 0, 0, 0, 0, 0, 0, 1)
}

#[no_mangle]
pub unsafe extern "C" fn gpuRenderPipelineCreateEx(
    vs: usize,
    fs: usize,
    topology: i32,
    cull: i32,
    ff: i32,
    de: i32,
    dw: i32,
    dc: i32,
    be: i32,
    sc: i32,
) -> i32 {
    render::pipeline_create_ex(
        &read_string(vs),
        &read_string(fs),
        topology,
        cull,
        ff,
        de,
        dw,
        dc,
        be,
        sc,
    )
}

#[no_mangle]
pub extern "C" fn gpuRenderPipelineDestroy(id: i32) {
    render::pipeline_destroy(id);
}

#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn gpuSamplerCreate(
    mag_filter: i32,
    min_filter: i32,
    mip_filter: i32,
    address_u: i32,
    address_v: i32,
    address_w: i32,
    lod_min: f32,
    lod_max: f32,
    compare: i32,
    max_anisotropy: i32,
) -> i32 {
    textures::sampler_create(
        mag_filter,
        min_filter,
        mip_filter,
        address_u,
        address_v,
        address_w,
        lod_min,
        lod_max,
        compare,
        max_anisotropy,
    )
}

#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn gpuTextureCreate(
    format: i32,
    dimension: i32,
    width: i32,
    height: i32,
    depth_or_layers: i32,
    mip_levels: i32,
    sample_count: i32,
    storage_access: i32,
    view_dimension: i32,
) -> i32 {
    textures::texture_create(
        format,
        dimension,
        width,
        height,
        depth_or_layers,
        mip_levels,
        sample_count,
        storage_access,
        view_dimension,
    )
}

#[no_mangle]
pub extern "C" fn gpuTextureDestroy(id: i32) {
    textures::texture_destroy(id);
}

#[no_mangle]
pub extern "C" fn gpuPassBegin(query_set: i32, ts_begin: i32, ts_end: i32) -> i32 {
    compute::pass_begin(query_set, ts_begin, ts_end)
}

#[no_mangle]
pub unsafe extern "C" fn gpuPassDispatch(
    pass: i32,
    kernel: usize,
    bufs: usize,
    tex: usize,
    samp: usize,
    ex: i32,
    ey: i32,
    ez: i32,
    uniforms: usize,
) {
    compute::pass_dispatch(
        pass,
        read_string(kernel),
        read_i32s(bufs),
        read_i32s(tex),
        read_i32s(samp),
        ex,
        ey,
        ez,
        read_bytes(uniforms),
    );
}

#[no_mangle]
pub extern "C" fn gpuPassSubmit(pass: i32) -> i32 {
    compute::pass_submit(pass)
}

#[no_mangle]
pub extern "C" fn gpuQuerySetCreateTimestamps(count: i32) -> i32 {
    crate::gpu::queries::create_timestamps(count)
}

#[no_mangle]
pub extern "C" fn gpuQuerySetDestroy(id: i32) {
    crate::gpu::queries::destroy(id);
}

#[no_mangle]
pub extern "C" fn gpuQuerySetRead(id: i32) -> usize {
    alloc_i64s(&crate::gpu::queries::read(id))
}

#[no_mangle]
pub extern "C" fn gpuTimestampPeriod() -> f32 {
    crate::gpu::queries::period()
}

#[no_mangle]
pub unsafe extern "C" fn gpuSurfaceFromCanvas(id: usize) -> i32 {
    surface::from_canvas(&read_string(id))
}

#[no_mangle]
pub extern "C" fn gpuSurfacePointer(id: i32) -> usize {
    alloc_bytes(&surface::pointer_bytes(id))
}

#[no_mangle]
pub extern "C" fn gpuSurfacePointers(id: i32) -> usize {
    alloc_bytes(&surface::pointers_bytes(id))
}

#[no_mangle]
pub extern "C" fn gpuSurfacePixelRatio(id: i32) -> f32 {
    surface::pixel_ratio(id)
}

#[no_mangle]
pub extern "C" fn gpuSurfaceScaleFactor(id: i32) -> f32 {
    surface::scale_factor(id)
}

#[no_mangle]
pub extern "C" fn gpuSurfaceRequestPointerLock(id: i32) {
    surface::request_pointer_lock(id);
}

#[no_mangle]
pub extern "C" fn gpuSurfaceExitPointerLock(id: i32) {
    surface::exit_pointer_lock(id);
}

#[no_mangle]
pub extern "C" fn gpuSurfacePointerLocked(id: i32) -> i32 {
    i32::from(surface::pointer_locked(id))
}

#[no_mangle]
pub extern "C" fn gpuSurfaceRequestFullscreen(id: i32) {
    surface::request_fullscreen(id);
}

#[no_mangle]
pub extern "C" fn gpuSurfaceExitFullscreen(id: i32) {
    surface::exit_fullscreen(id);
}

#[no_mangle]
pub extern "C" fn gpuSurfaceFullscreen(id: i32) -> i32 {
    i32::from(surface::fullscreen(id))
}

#[no_mangle]
pub extern "C" fn gpuSurfaceMods(id: i32) -> usize {
    alloc_bytes(&surface::mods_bytes(id))
}

#[no_mangle]
pub unsafe extern "C" fn gpuSurfaceKeyDown(id: i32, code: usize) -> i32 {
    i32::from(surface::key_down(id, &read_string(code)))
}

#[no_mangle]
pub extern "C" fn gpuSurfaceGamepads(id: i32) -> usize {
    alloc_i32s(&surface::gamepads(id))
}

#[no_mangle]
pub extern "C" fn gpuSurfaceGamepadConnected(id: i32, pad: i32) -> i32 {
    i32::from(surface::gamepad_connected(id, pad))
}

#[no_mangle]
pub extern "C" fn gpuSurfaceGamepadButtonDown(id: i32, pad: i32, button: i32) -> i32 {
    i32::from(surface::gamepad_button_down(id, pad, button))
}

#[no_mangle]
pub extern "C" fn gpuSurfaceGamepadAxis(id: i32, pad: i32, axis: i32) -> f32 {
    surface::gamepad_axis(id, pad, axis)
}

#[no_mangle]
pub extern "C" fn gpuSurfacePollEvents(id: i32) -> usize {
    alloc_bytes(&surface::poll_events_bytes(id))
}

#[no_mangle]
pub extern "C" fn gpuRenderBlit(sid: i32, tid: i32) -> i32 {
    surface::blit(sid, tid)
}

#[no_mangle]
pub unsafe extern "C" fn gpuShaderFromWgsl(source: usize, entry: usize) -> i32 {
    compute::shader_from_wgsl(read_string(source), read_string(entry))
}

#[no_mangle]
pub unsafe extern "C" fn gpuDispatchShader(
    shader_id: i32,
    bufs: usize,
    wx: i32,
    wy: i32,
    wz: i32,
) -> i32 {
    compute::dispatch_shader(shader_id, &read_i32s(bufs), wx, wy, wz)
}

#[no_mangle]
pub unsafe extern "C" fn gpuTextureFromImageBytes(pixels: usize) -> usize {
    alloc_i32s(&textures::from_image_bytes(read_bytes(pixels)))
}

#[no_mangle]
pub unsafe extern "C" fn gpuTextureWriteRgba(
    id: i32,
    pixels: usize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> i32 {
    textures::texture_write_rgba(id, read_bytes(pixels), x, y, w, h)
}

#[no_mangle]
pub extern "C" fn gpuTextureReadRgba(id: i32) -> usize {
    alloc_bytes(&textures::texture_read_rgba(id))
}

#[no_mangle]
pub extern "C" fn gpuTextureCopyFromBuffer(
    tex_id: i32,
    buf_id: i32,
    byte_offset: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) {
    textures::texture_copy_from_buffer(tex_id, buf_id, byte_offset, x, y, w, h);
}

#[no_mangle]
pub extern "C" fn gpuTextureCopyToBuffer(
    tex_id: i32,
    buf_id: i32,
    byte_offset: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) {
    textures::texture_copy_to_buffer(tex_id, buf_id, byte_offset, x, y, w, h);
}

#[no_mangle]
pub extern "C" fn gpuSamplerDestroy(id: i32) {
    textures::sampler_destroy(id);
}

#[no_mangle]
pub extern "C" fn gpuTextureCopy(
    src_id: i32,
    dst_id: i32,
    src_x: i32,
    src_y: i32,
    dst_x: i32,
    dst_y: i32,
    width: i32,
    height: i32,
) {
    textures::texture_copy(src_id, dst_id, src_x, src_y, dst_x, dst_y, width, height);
}

#[no_mangle]
pub extern "C" fn gpuTextureGenerateMipmaps(id: i32) -> i32 {
    textures::texture_generate_mipmaps(id)
}

#[no_mangle]
pub unsafe extern "C" fn gpuPassDispatchIndirect(
    pass: i32,
    kernel: usize,
    bufs: usize,
    tex: usize,
    samp: usize,
    indirect: i32,
    off: i32,
) {
    compute::pass_dispatch_indirect(
        pass,
        read_string(kernel),
        read_i32s(bufs),
        read_i32s(tex),
        read_i32s(samp),
        indirect,
        off,
    );
}

#[no_mangle]
pub extern "C" fn gpuPassWriteTimestamp(pass: i32, query_set: i32, index: i32) {
    compute::pass_write_timestamp(pass, query_set, index);
}

#[no_mangle]
pub unsafe extern "C" fn gpuEncoderSubmit(stream: usize) -> i32 {
    crate::gpu::encoder::submit(&read_bytes(stream))
}

#[no_mangle]
pub unsafe extern "C" fn gpuBindGroupCreate(
    pid: i32,
    group: i32,
    bufs: usize,
    tex: usize,
    samp: usize,
) -> i32 {
    render::bind_group_create(
        pid,
        group,
        &read_i32s(bufs),
        &read_i32s(tex),
        &read_i32s(samp),
    )
}

#[no_mangle]
pub extern "C" fn gpuBindGroupDestroy(id: i32) {
    render::bind_group_destroy(id);
}

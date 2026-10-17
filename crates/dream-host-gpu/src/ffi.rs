//! Declarations of the C++ glue in `native/glue/dgpu.h`.

use std::ffi::{c_char, c_int, c_void};

pub const OK: c_int = 0;
pub const FAILED: c_int = 1;
pub const INVALID: c_int = 2;
pub const UNSUPPORTED: c_int = 3;
pub const OUT_OF_MEMORY: c_int = 4;
pub const DEVICE_LOST: c_int = 5;
pub const SURFACE_LOST: c_int = 6;
pub const TIMEOUT: c_int = 7;

#[repr(C)]
pub struct DgpuDevice {
    _opaque: [u8; 0],
}
#[repr(C)]
pub struct DgpuProgram {
    _opaque: [u8; 0],
}
#[repr(C)]
pub struct DgpuPipeline {
    _opaque: [u8; 0],
}
#[repr(C)]
pub struct DgpuSurface {
    _opaque: [u8; 0],
}
#[repr(C)]
pub struct DgpuEncoder {
    _opaque: [u8; 0],
}
pub type DgpuObject = c_void;

#[repr(C)]
pub struct TextureDesc {
    pub width: u32,
    pub height: u32,
    pub layers: u32,
    pub mips: u32,
    pub format: u32,
    pub usage: u32,
    pub samples: u32,
}

#[repr(C)]
#[derive(Default)]
pub struct ReadbackLayout {
    pub staging_size: u64,
    pub row_pitch: u64,
    pub row_bytes: u64,
    pub rows: u32,
}

#[repr(C)]
pub struct EntryCode {
    pub name: *const c_char,
    pub code: *const c_void,
    pub size: usize,
}

#[repr(C)]
pub struct Param {
    pub name: *const c_char,
    pub kind: i32,
    pub flags: u32,
    pub space: u32,
    pub binding: u32,
    pub offset: u32,
    pub size: u32,
}

#[repr(C)]
pub struct StageParams {
    pub params: *const Param,
    pub count: u32,
}

#[repr(C)]
pub struct VertexAttribute {
    pub location: u32,
    pub format: u32,
    pub offset: u32,
    pub slot: u32,
}

#[repr(C)]
pub struct VertexBufferLayout {
    pub stride: u32,
    pub per_instance: u32,
}

#[repr(C)]
pub struct RenderDesc {
    pub color_format: u32,
    pub topology: u32,
    pub cull: u32,
    pub blend: u32,
    pub depth_format: u32,
    pub depth_compare: u32,
    pub depth_write: u32,
    pub samples: u32,
    pub buffers: *const VertexBufferLayout,
    pub buffer_count: u32,
    pub attributes: *const VertexAttribute,
    pub attribute_count: u32,
}

#[repr(C)]
pub struct HitGroup {
    pub closest_hit: *const c_char,
    pub any_hit: *const c_char,
    pub intersection: *const c_char,
}

#[repr(C)]
pub struct RayTracingDesc {
    pub ray_generation: *const c_char,
    pub misses: *const *const c_char,
    pub miss_count: u32,
    pub hit_groups: *const HitGroup,
    pub hit_group_count: u32,
    pub callables: *const *const c_char,
    pub callable_count: u32,
    pub max_payload_size: u32,
    pub max_attribute_size: u32,
    pub max_recursion: u32,
}

#[repr(C)]
pub struct Geometry {
    pub kind: i32,
    pub vertices: *mut DgpuObject,
    pub vertex_count: u32,
    pub vertex_stride: u32,
    pub indices: *mut DgpuObject,
    pub index_count: u32,
    pub opaque: i32,
}

#[repr(C)]
pub struct Instance {
    pub bottom_level: *mut DgpuObject,
    pub transform: [f32; 12],
    pub id: u32,
    pub mask: u32,
    pub hit_group_offset: u32,
    pub flags: u32,
}

#[repr(C)]
pub struct Window {
    pub platform: u32,
    pub window: *mut c_void,
    pub display: *mut c_void,
    pub xlib_window: u64,
}

unsafe extern "C" {
    pub fn dgpu_message() -> *const c_char;
    pub fn dgpu_backend_compiled(backend: c_int) -> c_int;
    pub fn dgpu_device_create(backend: c_int, out: *mut *mut DgpuDevice) -> c_int;
    pub fn dgpu_device_destroy(device: *mut DgpuDevice);
    pub fn dgpu_device_features(device: *mut DgpuDevice) -> u32;
    pub fn dgpu_device_lost(device: *mut DgpuDevice) -> c_int;
    pub fn dgpu_fence_wait(device: *mut DgpuDevice, value: u64, timeout_ns: u64) -> c_int;
    pub fn dgpu_object_release(object: *mut DgpuObject);

    pub fn dgpu_buffer_create(
        device: *mut DgpuDevice,
        size: u64,
        usage: u32,
        readback: c_int,
        out: *mut *mut DgpuObject,
    ) -> c_int;
    pub fn dgpu_buffer_read_mapped(
        device: *mut DgpuDevice,
        buffer: *mut DgpuObject,
        out: *mut c_void,
        row_pitch: u64,
        row_bytes: u64,
        rows: u32,
    ) -> c_int;
    pub fn dgpu_texture_create(
        device: *mut DgpuDevice,
        desc: *const TextureDesc,
        out: *mut *mut DgpuObject,
    ) -> c_int;
    pub fn dgpu_texture_readback_layout(
        texture: *mut DgpuObject,
        mip: u32,
        out: *mut ReadbackLayout,
    ) -> c_int;
    pub fn dgpu_sampler_create(
        device: *mut DgpuDevice,
        desc: *const i32,
        out: *mut *mut DgpuObject,
    ) -> c_int;
    pub fn dgpu_descriptor_handle(
        device: *mut DgpuDevice,
        resource: *mut DgpuObject,
        kind: c_int,
        out: *mut u64,
    ) -> c_int;

    pub fn dgpu_program_create(
        device: *mut DgpuDevice,
        layout: *const c_void,
        layout_size: usize,
        entries: *const EntryCode,
        entry_count: u32,
        out: *mut *mut DgpuProgram,
    ) -> c_int;
    pub fn dgpu_program_release(program: *mut DgpuProgram);
    pub fn dgpu_compute_pipeline_create(
        device: *mut DgpuDevice,
        program: *mut DgpuProgram,
        stage: *const StageParams,
        out: *mut *mut DgpuPipeline,
    ) -> c_int;
    pub fn dgpu_render_pipeline_create(
        device: *mut DgpuDevice,
        program: *mut DgpuProgram,
        stages: *const StageParams,
        desc: *const RenderDesc,
        out: *mut *mut DgpuPipeline,
    ) -> c_int;
    pub fn dgpu_pipeline_release(pipeline: *mut DgpuPipeline);

    pub fn dgpu_encoder_begin(device: *mut DgpuDevice, out: *mut *mut DgpuEncoder) -> c_int;
    pub fn dgpu_encoder_abort(encoder: *mut DgpuEncoder);
    pub fn dgpu_encoder_submit(encoder: *mut DgpuEncoder, fence_value: *mut u64) -> c_int;
    pub fn dgpu_enc_write_buffer(
        e: *mut DgpuEncoder,
        buffer: *mut DgpuObject,
        offset: u64,
        data: *const c_void,
        size: u64,
    ) -> c_int;
    pub fn dgpu_enc_write_texture(
        e: *mut DgpuEncoder,
        texture: *mut DgpuObject,
        mip: u32,
        layer: u32,
        data: *const c_void,
        size: u64,
    ) -> c_int;
    pub fn dgpu_enc_copy_buffer(
        e: *mut DgpuEncoder,
        dst: *mut DgpuObject,
        dst_offset: u64,
        src: *mut DgpuObject,
        src_offset: u64,
        size: u64,
    ) -> c_int;
    pub fn dgpu_enc_read_texture(
        e: *mut DgpuEncoder,
        texture: *mut DgpuObject,
        mip: u32,
        layer: u32,
        readback: *mut DgpuObject,
    ) -> c_int;
    pub fn dgpu_enc_begin_render_pass(
        e: *mut DgpuEncoder,
        color: *mut DgpuObject,
        clear_color: c_int,
        rgba: *const f32,
        depth: *mut DgpuObject,
        clear_depth: c_int,
        depth_value: f32,
    ) -> c_int;
    pub fn dgpu_enc_begin_compute_pass(e: *mut DgpuEncoder) -> c_int;
    pub fn dgpu_enc_end_pass(e: *mut DgpuEncoder) -> c_int;
    pub fn dgpu_enc_set_pipeline(e: *mut DgpuEncoder, pipeline: *mut DgpuPipeline) -> c_int;
    pub fn dgpu_enc_bind(
        e: *mut DgpuEncoder,
        entry: u32,
        index: u32,
        resource: *mut DgpuObject,
        data: *const c_void,
        size: u32,
    ) -> c_int;
    pub fn dgpu_enc_set_vertex_buffer(
        e: *mut DgpuEncoder,
        slot: u32,
        buffer: *mut DgpuObject,
        offset: u64,
    ) -> c_int;
    pub fn dgpu_enc_set_index_buffer(
        e: *mut DgpuEncoder,
        buffer: *mut DgpuObject,
        format: u32,
        offset: u64,
    ) -> c_int;
    pub fn dgpu_enc_set_viewport(e: *mut DgpuEncoder, xywh_min_max: *const f32) -> c_int;
    pub fn dgpu_enc_set_scissor(e: *mut DgpuEncoder, x: i32, y: i32, w: i32, h: i32) -> c_int;
    pub fn dgpu_enc_draw(e: *mut DgpuEncoder, v: u32, i: u32, fv: u32, fi: u32) -> c_int;
    pub fn dgpu_enc_draw_indexed(
        e: *mut DgpuEncoder,
        indices: u32,
        instances: u32,
        first_index: u32,
        base_vertex: i32,
        first_instance: u32,
    ) -> c_int;
    pub fn dgpu_enc_dispatch(e: *mut DgpuEncoder, x: u32, y: u32, z: u32) -> c_int;
    pub fn dgpu_enc_trace_rays(e: *mut DgpuEncoder, width: u32, height: u32, depth: u32) -> c_int;

    pub fn dgpu_ray_tracing_pipeline_create(
        device: *mut DgpuDevice,
        program: *mut DgpuProgram,
        stages: *const StageParams,
        stage_count: u32,
        desc: *const RayTracingDesc,
        out: *mut *mut DgpuPipeline,
    ) -> c_int;
    pub fn dgpu_enc_build_bottom_level(
        e: *mut DgpuEncoder,
        geometry: *const Geometry,
        out: *mut *mut DgpuObject,
        scratch: *mut *mut DgpuObject,
    ) -> c_int;
    pub fn dgpu_enc_build_top_level(
        e: *mut DgpuEncoder,
        instances: *const Instance,
        count: u32,
        out: *mut *mut DgpuObject,
        scratch: *mut *mut DgpuObject,
        instance_buffer: *mut *mut DgpuObject,
    ) -> c_int;
    pub fn dgpu_object_retain(object: *mut DgpuObject);

    pub fn dgpu_surface_create(
        device: *mut DgpuDevice,
        window: *const Window,
        out: *mut *mut DgpuSurface,
    ) -> c_int;
    pub fn dgpu_surface_configure(
        surface: *mut DgpuSurface,
        width: u32,
        height: u32,
        vsync: c_int,
        format: *mut u32,
    ) -> c_int;
    pub fn dgpu_surface_acquire(surface: *mut DgpuSurface, texture: *mut *mut DgpuObject) -> c_int;
    pub fn dgpu_surface_present(surface: *mut DgpuSurface) -> c_int;
    pub fn dgpu_surface_release(surface: *mut DgpuSurface);

    pub fn dgpu_loaded_slang_compilers() -> c_int;
}

/// The calling thread's last glue failure detail.
pub fn message() -> String {
    let text = unsafe { dgpu_message() };
    if text.is_null() {
        return String::new();
    }
    unsafe { std::ffi::CStr::from_ptr(text) }
        .to_string_lossy()
        .into_owned()
}

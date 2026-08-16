//! `extern "C"` GPU hosts. Async ops return the payload (code / `char[]` / `i64`), not a Future.

#![allow(clippy::too_many_arguments)]

use super::{buffers, compute, device, error, load_abi_from_env, render, state, surface, textures};
use crate::guest;
use std::sync::OnceLock;
use std::time::Instant;

static GPU_TIME_ORIGIN: OnceLock<Instant> = OnceLock::new();

fn load() {
    load_abi_from_env();
}

#[no_mangle]
pub extern "C" fn dream_gpu_is_available() -> i32 {
    load();
    i32::from(device::is_available())
}

#[no_mangle]
pub extern "C" fn dream_gpu_ready() -> i32 {
    load();
    i32::from(state::lock_state().ready)
}

#[no_mangle]
pub extern "C" fn dream_gpu_last_error() -> i32 {
    load();
    guest::intern(&error::take_last_error())
}

#[no_mangle]
pub extern "C" fn dream_gpu_try_init() -> i32 {
    load();
    device::try_init()
}

#[no_mangle]
pub extern "C" fn dream_gpu_frame() -> i32 {
    load();
    surface::wait_display_frame();
    super::profile::end_frame();
    0
}

#[no_mangle]
pub extern "C" fn dream_gpu_timestamp() -> i64 {
    load();
    let origin = GPU_TIME_ORIGIN.get_or_init(Instant::now);
    origin.elapsed().as_nanos() as i64
}

#[no_mangle]
pub extern "C" fn dream_gpu_buffer_alloc_bytes(n: i32) -> i32 {
    load();
    buffers::alloc_bytes(n)
}

#[no_mangle]
pub extern "C" fn dream_gpu_buffer_alloc_vertex_bytes(n: i32) -> i32 {
    load();
    buffers::alloc_vertex_bytes(n)
}

#[no_mangle]
pub extern "C" fn dream_gpu_buffer_write_bytes(id: i32, data_ptr: i32) {
    load();
    let _ = buffers::write_bytes(id, guest::read_bytes(data_ptr));
}

#[no_mangle]
pub extern "C" fn dream_gpu_buffer_write_bytes_at(id: i32, byte_offset: i32, data_ptr: i32) {
    load();
    let _ = buffers::write_bytes_at(id, byte_offset, guest::read_bytes(data_ptr));
}

#[no_mangle]
pub extern "C" fn dream_gpu_buffer_read_bytes(id: i32, n: i32) -> i32 {
    load();
    match buffers::read_bytes(id, n) {
        Ok(b) => guest::write_bytes(&b),
        Err(_) => 0,
    }
}

#[no_mangle]
pub extern "C" fn dream_gpu_buffer_read_bytes_at(id: i32, byte_offset: i32, n: i32) -> i32 {
    load();
    match buffers::read_bytes_at(id, byte_offset, n) {
        Ok(b) => guest::write_bytes(&b),
        Err(_) => 0,
    }
}

#[no_mangle]
pub extern "C" fn dream_gpu_buffer_copy(src_id: i32, dst_id: i32, src_off: i32, dst_off: i32, size: i32) {
    load();
    buffers::copy(src_id, dst_id, src_off, dst_off, size);
}

#[no_mangle]
pub extern "C" fn dream_gpu_buffer_destroy(id: i32) {
    load();
    buffers::destroy(id);
}

#[no_mangle]
pub extern "C" fn dream_gpu_dispatch(
    kernel_ptr: i32,
    bufs: i32,
    tex: i32,
    samp: i32,
    ex: i32,
    ey: i32,
    ez: i32,
    uniforms_ptr: i32,
) -> i32 {
    load();
    let kernel = guest::read_string(kernel_ptr);
    let buffer_ids = guest::read_i32_array(bufs);
    let texture_ids = guest::read_i32_array(tex);
    let sampler_ids = guest::read_i32_array(samp);
    let uniforms = guest::read_bytes(uniforms_ptr);
    compute::dispatch(
        &kernel,
        &buffer_ids,
        &texture_ids,
        &sampler_ids,
        ex,
        ey,
        ez,
        &uniforms,
    )
}

#[no_mangle]
pub extern "C" fn dream_gpu_dispatch_indirect(
    kernel_ptr: i32,
    bufs: i32,
    tex: i32,
    samp: i32,
    indirect: i32,
    off: i32,
) -> i32 {
    load();
    let kernel = guest::read_string(kernel_ptr);
    let buffer_ids = guest::read_i32_array(bufs);
    let texture_ids = guest::read_i32_array(tex);
    let sampler_ids = guest::read_i32_array(samp);
    compute::dispatch_indirect(
        &kernel,
        &buffer_ids,
        &texture_ids,
        &sampler_ids,
        indirect,
        off,
    )
}

#[no_mangle]
pub extern "C" fn dream_gpu_shader_from_wgsl(source_ptr: i32, entry_ptr: i32) -> i32 {
    load();
    compute::shader_from_wgsl(guest::read_string(source_ptr), guest::read_string(entry_ptr))
}

#[no_mangle]
pub extern "C" fn dream_gpu_dispatch_shader(
    shader_id: i32,
    bufs: i32,
    wx: i32,
    wy: i32,
    wz: i32,
) -> i32 {
    load();
    let buffer_ids = guest::read_i32_array(bufs);
    compute::dispatch_shader(shader_id, &buffer_ids, wx, wy, wz)
}

#[no_mangle]
pub extern "C" fn dream_gpu_sampler_create(filter: i32) -> i32 {
    load();
    textures::sampler_create(filter)
}

#[no_mangle]
pub extern "C" fn dream_gpu_sampler_create_ex(filter: i32, address: i32, mip: i32) -> i32 {
    load();
    textures::sampler_create_ex(filter, address, mip)
}

#[no_mangle]
pub extern "C" fn dream_gpu_texture_create_rgba8(w: i32, h: i32) -> i32 {
    load();
    textures::texture_create_rgba8(w, h)
}

#[no_mangle]
pub extern "C" fn dream_gpu_texture_create_depth(w: i32, h: i32) -> i32 {
    load();
    textures::texture_create_depth(w, h)
}

#[no_mangle]
pub extern "C" fn dream_gpu_texture_create_rgba16_float(w: i32, h: i32) -> i32 {
    load();
    textures::texture_create_rgba16float(w, h)
}

#[no_mangle]
pub extern "C" fn dream_gpu_texture_create_cube_rgba8(size: i32) -> i32 {
    load();
    textures::texture_create_cube_rgba8(size)
}

#[no_mangle]
pub extern "C" fn dream_gpu_texture_write_rgba(
    id: i32,
    pixels_ptr: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> i32 {
    load();
    textures::texture_write_rgba(id, guest::read_bytes(pixels_ptr), x, y, w, h)
}

#[no_mangle]
pub extern "C" fn dream_gpu_texture_read_rgba(id: i32) -> i32 {
    load();
    guest::write_bytes(&textures::texture_read_rgba(id))
}

#[no_mangle]
pub extern "C" fn dream_gpu_texture_copy_from_buffer(
    tex_id: i32,
    buf_id: i32,
    byte_offset: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) {
    load();
    textures::texture_copy_from_buffer(tex_id, buf_id, byte_offset, x, y, w, h);
}

#[no_mangle]
pub extern "C" fn dream_gpu_texture_copy_to_buffer(
    tex_id: i32,
    buf_id: i32,
    byte_offset: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) {
    load();
    textures::texture_copy_to_buffer(tex_id, buf_id, byte_offset, x, y, w, h);
}

#[no_mangle]
pub extern "C" fn dream_gpu_texture_copy(
    src: i32,
    dst: i32,
    sx: i32,
    sy: i32,
    dx: i32,
    dy: i32,
    w: i32,
    h: i32,
) {
    load();
    textures::texture_copy(src, dst, sx, sy, dx, dy, w, h);
}

#[no_mangle]
pub extern "C" fn dream_gpu_texture_generate_mipmaps(id: i32) -> i32 {
    load();
    textures::texture_generate_mipmaps(id)
}

#[no_mangle]
pub extern "C" fn dream_gpu_texture_destroy(id: i32) {
    load();
    textures::texture_destroy(id);
}

#[no_mangle]
pub extern "C" fn dream_gpu_sampler_destroy(id: i32) {
    load();
    textures::sampler_destroy(id);
}

#[no_mangle]
pub extern "C" fn dream_gpu_surface_from_canvas(id_ptr: i32) -> i32 {
    load();
    let name = guest::read_string(id_ptr);
    surface::from_canvas(&name)
}

#[no_mangle]
pub extern "C" fn dream_gpu_surface_create(title_ptr: i32, width: i32, height: i32) -> i32 {
    load();
    let title = guest::read_string(title_ptr);
    surface::create(&title, width, height)
}

#[no_mangle]
pub extern "C" fn dream_gpu_surface_configure(id: i32, w: i32, h: i32) {
    load();
    surface::configure(id, w, h);
}

#[no_mangle]
pub extern "C" fn dream_gpu_surface_present(id: i32) -> i32 {
    load();
    surface::present(id)
}

#[no_mangle]
pub extern "C" fn dream_gpu_surface_pointer(id: i32) -> i32 {
    load();
    guest::write_bytes(&surface::pointer_bytes(id))
}

#[no_mangle]
pub extern "C" fn dream_gpu_surface_mods(id: i32) -> i32 {
    load();
    guest::write_bytes(&surface::mods_bytes(id))
}

#[no_mangle]
pub extern "C" fn dream_gpu_surface_key_down(id: i32, code_ptr: i32) -> i32 {
    load();
    let code = guest::read_string(code_ptr);
    i32::from(surface::key_down(id, &code))
}

#[no_mangle]
pub extern "C" fn dream_gpu_surface_gamepads(id: i32) -> i32 {
    load();
    guest::write_i32_array(&surface::gamepads(id))
}

#[no_mangle]
pub extern "C" fn dream_gpu_surface_gamepad_connected(id: i32, pad: i32) -> i32 {
    load();
    i32::from(surface::gamepad_connected(id, pad))
}

#[no_mangle]
pub extern "C" fn dream_gpu_surface_gamepad_button_down(id: i32, pad: i32, button: i32) -> i32 {
    load();
    i32::from(surface::gamepad_button_down(id, pad, button))
}

#[no_mangle]
pub extern "C" fn dream_gpu_surface_gamepad_axis(id: i32, pad: i32, axis: i32) -> f32 {
    load();
    surface::gamepad_axis(id, pad, axis)
}

#[no_mangle]
pub extern "C" fn dream_gpu_surface_focused(id: i32) -> i32 {
    load();
    i32::from(surface::focused(id))
}

#[no_mangle]
pub extern "C" fn dream_gpu_surface_close_requested(id: i32) -> i32 {
    load();
    i32::from(surface::close_requested(id))
}

#[no_mangle]
pub extern "C" fn dream_gpu_surface_poll_events(id: i32) -> i32 {
    load();
    guest::write_bytes(&surface::poll_events_bytes(id))
}

#[no_mangle]
pub extern "C" fn dream_gpu_surface_width(id: i32) -> i32 {
    load();
    surface::width(id)
}

#[no_mangle]
pub extern "C" fn dream_gpu_surface_height(id: i32) -> i32 {
    load();
    surface::height(id)
}

#[no_mangle]
pub extern "C" fn dream_gpu_render_blit(sid: i32, tid: i32) -> i32 {
    load();
    surface::blit(sid, tid)
}

#[no_mangle]
pub extern "C" fn dream_gpu_render_pipeline_create(vs: i32, fs: i32) -> i32 {
    load();
    let vs_name = guest::read_string(vs);
    let fs_name = guest::read_string(fs);
    render::pipeline_create_ex(&vs_name, &fs_name, 0, 0, 0, 0, 0, 0, 0, 1)
}

#[no_mangle]
pub extern "C" fn dream_gpu_render_pipeline_create_ex(
    vs: i32,
    fs: i32,
    topology: i32,
    cull: i32,
    ff: i32,
    de: i32,
    dw: i32,
    dc: i32,
    be: i32,
    sc: i32,
) -> i32 {
    load();
    let vs_name = guest::read_string(vs);
    let fs_name = guest::read_string(fs);
    render::pipeline_create_ex(&vs_name, &fs_name, topology, cull, ff, de, dw, dc, be, sc)
}

#[no_mangle]
pub extern "C" fn dream_gpu_render_draw(
    sid: i32,
    pid: i32,
    vb: i32,
    n: i32,
    uniforms: i32,
    cr: f32,
    cg: f32,
    cb: f32,
    ca: f32,
) -> i32 {
    load();
    let u = guest::read_bytes(uniforms);
    render::draw_ex(sid, pid, vb, n, 1, &u, [cr, cg, cb, ca], -1, 0, None)
}

#[no_mangle]
pub extern "C" fn dream_gpu_render_draw_ex(
    sid: i32,
    pid: i32,
    vb: i32,
    n: i32,
    inst: i32,
    uniforms: i32,
    cr: f32,
    cg: f32,
    cb: f32,
    ca: f32,
    depth: i32,
    load_op: i32,
) -> i32 {
    load();
    let u = guest::read_bytes(uniforms);
    render::draw_ex(
        sid,
        pid,
        vb,
        n,
        inst,
        &u,
        [cr, cg, cb, ca],
        depth,
        load_op,
        None,
    )
}

#[no_mangle]
pub extern "C" fn dream_gpu_render_draw_indexed(
    sid: i32,
    pid: i32,
    vb: i32,
    ib: i32,
    n: i32,
    uniforms: i32,
    cr: f32,
    cg: f32,
    cb: f32,
    ca: f32,
) -> i32 {
    load();
    let u = guest::read_bytes(uniforms);
    render::draw_ex(
        sid,
        pid,
        vb,
        0,
        1,
        &u,
        [cr, cg, cb, ca],
        -1,
        0,
        Some((ib, n)),
    )
}

#[no_mangle]
pub extern "C" fn dream_gpu_render_draw_to(
    color_tex: i32,
    pid: i32,
    vb: i32,
    n: i32,
    inst: i32,
    uniforms: i32,
    cr: f32,
    cg: f32,
    cb: f32,
    ca: f32,
    depth: i32,
    load_op: i32,
) -> i32 {
    load();
    let u = guest::read_bytes(uniforms);
    render::draw_to(
        color_tex,
        pid,
        vb,
        n,
        inst,
        &u,
        [cr, cg, cb, ca],
        depth,
        load_op,
    )
}

#[no_mangle]
pub extern "C" fn dream_gpu_render_pipeline_destroy(id: i32) {
    load();
    render::pipeline_destroy(id);
}

#[no_mangle]
pub extern "C" fn dream_gpu_surface_destroy(id: i32) {
    load();
    surface::destroy(id);
}

#[no_mangle]
pub extern "C" fn dream_gpu_render_draw_indexed_ex(
    sid: i32,
    pid: i32,
    vb: i32,
    ib: i32,
    n: i32,
    inst: i32,
    uniforms: i32,
    cr: f32,
    cg: f32,
    cb: f32,
    ca: f32,
    depth: i32,
    load_op: i32,
) -> i32 {
    load();
    let u = guest::read_bytes(uniforms);
    render::draw_ex(
        sid,
        pid,
        vb,
        0,
        inst,
        &u,
        [cr, cg, cb, ca],
        depth,
        load_op,
        Some((ib, n)),
    )
}

#[no_mangle]
pub extern "C" fn dream_gpu_pass_begin() -> i32 {
    load();
    compute::pass_begin()
}

#[no_mangle]
pub extern "C" fn dream_gpu_pass_dispatch(
    pass: i32,
    kernel_ptr: i32,
    bufs: i32,
    tex: i32,
    samp: i32,
    ex: i32,
    ey: i32,
    ez: i32,
    uniforms_ptr: i32,
) {
    load();
    let kernel = guest::read_string(kernel_ptr);
    let buffer_ids = guest::read_i32_array(bufs);
    let texture_ids = guest::read_i32_array(tex);
    let sampler_ids = guest::read_i32_array(samp);
    let uniforms = guest::read_bytes(uniforms_ptr);
    compute::pass_dispatch(
        pass,
        kernel,
        buffer_ids,
        texture_ids,
        sampler_ids,
        ex,
        ey,
        ez,
        uniforms,
    );
}

#[no_mangle]
pub extern "C" fn dream_gpu_pass_dispatch_indirect(
    pass: i32,
    kernel_ptr: i32,
    bufs: i32,
    tex: i32,
    samp: i32,
    indirect: i32,
    off: i32,
) {
    load();
    let kernel = guest::read_string(kernel_ptr);
    let buffer_ids = guest::read_i32_array(bufs);
    let texture_ids = guest::read_i32_array(tex);
    let sampler_ids = guest::read_i32_array(samp);
    compute::pass_dispatch_indirect(
        pass,
        kernel,
        buffer_ids,
        texture_ids,
        sampler_ids,
        indirect,
        off,
    );
}

#[no_mangle]
pub extern "C" fn dream_gpu_pass_submit(pass_id: i32) -> i32 {
    load();
    compute::pass_submit(pass_id)
}

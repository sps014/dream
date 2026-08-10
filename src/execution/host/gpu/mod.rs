//! Native wgpu host for `system.gpu` (`Dream` wasm imports).

mod abi;
mod buffers;
mod compute;
mod device;
mod error;
mod gamepad;
mod icon;
mod input;
mod render;
mod state;
mod surface;
mod textures;

pub use icon::set_packaged_app_icon;

use std::path::Path;
use std::time::Instant;

use wasmtime::*;

use super::memory::{
    read_arg_bytes, read_arg_i32_array, read_arg_string, resolve_host_future_bytes,
    write_i32_array_to_memory, write_string_to_memory,
};
use dream_mir::abi as mir_abi;
use dream_mir::async_emit::{F_SLOTS, HOST_POLL_INDEX, KIND_HOST};
use state::lock_state;

fn call_export_2(caller: &mut Caller<'_, ()>, name: &str, a: i32, b: i32) -> Result<()> {
    let func = caller
        .get_export(name)
        .and_then(Extern::into_func)
        .ok_or_else(|| Error::msg(format!("module must export `{name}`")))?
        .typed::<(i32, i32), ()>(&*caller)
        .map_err(|_| Error::msg(format!("unexpected `{name}` signature")))?;
    func.call(&mut *caller, (a, b))?;
    Ok(())
}

fn resolve_host_future_i32(caller: &mut Caller<'_, ()>, value: i32) -> Result<i32> {
    let new_future = caller
        .get_export(mir_abi::EXPORT_NEW_FUTURE)
        .and_then(Extern::into_func)
        .ok_or_else(|| Error::msg("module must export `__dream_new_future`"))?
        .typed::<(i32, i32, i32), i32>(&*caller)
        .map_err(|_| Error::msg("unexpected `__dream_new_future` signature"))?;
    let future = new_future.call(&mut *caller, (F_SLOTS, HOST_POLL_INDEX, KIND_HOST))?;
    call_export_2(caller, mir_abi::EXPORT_RESOLVE, future, value)?;
    Ok(future)
}

fn resolve_host_future_void(caller: &mut Caller<'_, ()>) -> Result<i32> {
    resolve_host_future_i32(caller, 0)
}

/// Load sibling `.abi.json` `gpu` section into this thread's GPU state (for `dream run` / e2e).
/// Missing `gpu` metadata is recorded quietly; a warning is emitted only if kernels/shaders are used.
pub fn attach_abi_from_wat_path(wat_path: &str) {
    let path = Path::new(wat_path);
    let abi_path = path.with_extension("abi.json");
    let gpu = abi::load_gpu_abi_beside(path);
    let missing = if gpu.is_some() {
        None
    } else if !abi_path.exists() {
        Some(format!("sibling ABI missing ({})", abi_path.display()))
    } else {
        Some(format!("no `gpu` section in {}", abi_path.display()))
    };
    let mut st = lock_state();
    // Fresh slot per compile+run so parallel e2e cases never share ids/pipelines.
    st.reset();
    st.abi = gpu;
    st.missing_gpu_abi = missing;
    st.warned_missing_gpu_abi = false;
}

/// Link `Dream` gpu* imports used by `system.gpu`.
pub fn link_gpu_functions(linker: &mut Linker<()>) -> Result<()> {
    linker.func_wrap("Dream", "gpuIsAvailable", || -> i32 {
        i32::from(device::is_available())
    })?;
    linker.func_wrap("Dream", "gpuReady", || -> i32 {
        let st = lock_state();
        i32::from(st.ready)
    })?;
    linker.func_wrap(
        "Dream",
        "gpuLastError",
        |mut caller: Caller<'_, ()>| -> Result<i32> {
            // Consume so each Dream `GpuError.from_code` gets a fresh detail once.
            let msg = error::take_last_error();
            write_string_to_memory(&mut caller, &msg)
        },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuTryInit",
        |mut caller: Caller<'_, ()>| -> Result<i32> {
            let code = device::try_init();
            resolve_host_future_i32(&mut caller, code)
        },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuFrame",
        |mut caller: Caller<'_, ()>| -> Result<i32> {
            surface::frame_tick();
            resolve_host_future_void(&mut caller)
        },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuTimestamp",
        |mut caller: Caller<'_, ()>| -> Result<i32> {
            // Nanoseconds truncated into i32 slot — full i64 host path is not wired.
            let _ = Instant::now();
            resolve_host_future_i32(&mut caller, 0)
        },
    )?;

    linker.func_wrap("Dream", "gpuBufferAllocBytes", |n: i32| -> i32 {
        buffers::alloc_bytes(n)
    })?;
    linker.func_wrap("Dream", "gpuBufferAllocVertexBytes", |n: i32| -> i32 {
        buffers::alloc_vertex_bytes(n)
    })?;

    linker.func_wrap(
        "Dream",
        "gpuBufferWriteBytes",
        |mut caller: Caller<'_, ()>, id: i32, data_ptr: i32| -> Result<()> {
            let bytes = read_arg_bytes(&mut caller, data_ptr)?;
            buffers::write_bytes(id, bytes).map_err(Error::msg)
        },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuBufferWriteBytesAt",
        |mut caller: Caller<'_, ()>, id: i32, byte_offset: i32, data_ptr: i32| -> Result<()> {
            let bytes = read_arg_bytes(&mut caller, data_ptr)?;
            buffers::write_bytes_at(id, byte_offset, bytes).map_err(Error::msg)
        },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuBufferReadBytes",
        |mut caller: Caller<'_, ()>, id: i32, n: i32| -> Result<i32> {
            let bytes = buffers::read_bytes(id, n).map_err(Error::msg)?;
            resolve_host_future_bytes(&mut caller, &bytes)
        },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuBufferReadBytesAt",
        |mut caller: Caller<'_, ()>, id: i32, byte_offset: i32, n: i32| -> Result<i32> {
            let bytes = buffers::read_bytes_at(id, byte_offset, n).map_err(Error::msg)?;
            resolve_host_future_bytes(&mut caller, &bytes)
        },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuBufferCopy",
        |src_id: i32, dst_id: i32, src_off: i32, dst_off: i32, size: i32| {
            buffers::copy(src_id, dst_id, src_off, dst_off, size);
        },
    )?;

    linker.func_wrap(
        "Dream",
        "gpuDispatch",
        |mut caller: Caller<'_, ()>,
         kernel_ptr: i32,
         bufs: i32,
         tex: i32,
         samp: i32,
         ex: i32,
         ey: i32,
         ez: i32,
         uniforms_ptr: i32|
         -> Result<i32> {
            let kernel = read_arg_string(&mut caller, kernel_ptr)?;
            let buffer_ids = read_arg_i32_array(&mut caller, bufs)?;
            let texture_ids = read_arg_i32_array(&mut caller, tex)?;
            let sampler_ids = read_arg_i32_array(&mut caller, samp)?;
            let uniforms = read_arg_bytes(&mut caller, uniforms_ptr)?;
            let code = compute::dispatch(
                &kernel,
                &buffer_ids,
                &texture_ids,
                &sampler_ids,
                ex,
                ey,
                ez,
                &uniforms,
            );
            resolve_host_future_i32(&mut caller, code)
        },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuDispatchIndirect",
        |mut caller: Caller<'_, ()>,
         kernel_ptr: i32,
         bufs: i32,
         tex: i32,
         samp: i32,
         indirect: i32,
         off: i32|
         -> Result<i32> {
            let kernel = read_arg_string(&mut caller, kernel_ptr)?;
            let buffer_ids = read_arg_i32_array(&mut caller, bufs)?;
            let texture_ids = read_arg_i32_array(&mut caller, tex)?;
            let sampler_ids = read_arg_i32_array(&mut caller, samp)?;
            let code = compute::dispatch_indirect(
                &kernel,
                &buffer_ids,
                &texture_ids,
                &sampler_ids,
                indirect,
                off,
            );
            resolve_host_future_i32(&mut caller, code)
        },
    )?;

    linker.func_wrap(
        "Dream",
        "gpuShaderFromWgsl",
        |mut caller: Caller<'_, ()>, source_ptr: i32, entry_ptr: i32| -> Result<i32> {
            let source = read_arg_string(&mut caller, source_ptr)?;
            let entry = read_arg_string(&mut caller, entry_ptr)?;
            Ok(compute::shader_from_wgsl(source, entry))
        },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuDispatchShader",
        |mut caller: Caller<'_, ()>,
         shader_id: i32,
         bufs: i32,
         wx: i32,
         wy: i32,
         wz: i32|
         -> Result<i32> {
            let buffer_ids = read_arg_i32_array(&mut caller, bufs)?;
            let code = compute::dispatch_shader(shader_id, &buffer_ids, wx, wy, wz);
            resolve_host_future_i32(&mut caller, code)
        },
    )?;

    linker.func_wrap("Dream", "gpuSamplerCreate", |filter: i32| -> i32 {
        textures::sampler_create(filter)
    })?;
    linker.func_wrap(
        "Dream",
        "gpuSamplerCreateEx",
        |filter: i32, address: i32, mip: i32| -> i32 {
            textures::sampler_create_ex(filter, address, mip)
        },
    )?;
    linker.func_wrap("Dream", "gpuTextureCreateRgba8", |w: i32, h: i32| -> i32 {
        textures::texture_create_rgba8(w, h)
    })?;
    linker.func_wrap("Dream", "gpuTextureCreateDepth", |w: i32, h: i32| -> i32 {
        textures::texture_create_depth(w, h)
    })?;
    linker.func_wrap(
        "Dream",
        "gpuTextureCreateRgba16Float",
        |w: i32, h: i32| -> i32 { textures::texture_create_rgba16float(w, h) },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuTextureCreateCubeRgba8",
        |size: i32| -> i32 { textures::texture_create_cube_rgba8(size) },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuTextureWriteRgba",
        |mut caller: Caller<'_, ()>,
         id: i32,
         pixels_ptr: i32,
         x: i32,
         y: i32,
         w: i32,
         h: i32|
         -> Result<i32> {
            let pixels = read_arg_bytes(&mut caller, pixels_ptr)?;
            let code = textures::texture_write_rgba(id, pixels, x, y, w, h);
            resolve_host_future_i32(&mut caller, code)
        },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuTextureReadRgba",
        |mut caller: Caller<'_, ()>, id: i32| -> Result<i32> {
            let bytes = textures::texture_read_rgba(id);
            resolve_host_future_bytes(&mut caller, &bytes)
        },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuTextureCopyFromBuffer",
        |tex_id: i32, buf_id: i32, byte_offset: i32, x: i32, y: i32, w: i32, h: i32| {
            textures::texture_copy_from_buffer(tex_id, buf_id, byte_offset, x, y, w, h);
        },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuTextureCopyToBuffer",
        |tex_id: i32, buf_id: i32, byte_offset: i32, x: i32, y: i32, w: i32, h: i32| {
            textures::texture_copy_to_buffer(tex_id, buf_id, byte_offset, x, y, w, h);
        },
    )?;

    linker.func_wrap(
        "Dream",
        "gpuSurfaceFromCanvas",
        |mut caller: Caller<'_, ()>, id_ptr: i32| -> Result<i32> {
            let name = read_arg_string(&mut caller, id_ptr)?;
            Ok(surface::from_canvas(&name))
        },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuSurfaceCreate",
        |mut caller: Caller<'_, ()>, title_ptr: i32, width: i32, height: i32| -> Result<i32> {
            let title = read_arg_string(&mut caller, title_ptr)?;
            Ok(surface::create(&title, width, height))
        },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuSurfaceConfigure",
        |id: i32, w: i32, h: i32| {
            surface::configure(id, w, h);
        },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuSurfacePresent",
        |mut caller: Caller<'_, ()>, id: i32| -> Result<i32> {
            let code = surface::present(id);
            resolve_host_future_i32(&mut caller, code)
        },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuSurfacePointer",
        |mut caller: Caller<'_, ()>, id: i32| -> Result<i32> {
            let bytes = surface::pointer_bytes(id);
            super::memory::write_bytes_to_memory(&mut caller, &bytes)
        },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuSurfaceMods",
        |mut caller: Caller<'_, ()>, id: i32| -> Result<i32> {
            let bytes = surface::mods_bytes(id);
            super::memory::write_bytes_to_memory(&mut caller, &bytes)
        },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuSurfaceKeyDown",
        |mut caller: Caller<'_, ()>, id: i32, code_ptr: i32| -> Result<i32> {
            let code = read_arg_string(&mut caller, code_ptr)?;
            Ok(i32::from(surface::key_down(id, &code)))
        },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuSurfaceGamepads",
        |mut caller: Caller<'_, ()>, id: i32| -> Result<i32> {
            let pads = surface::gamepads(id);
            write_i32_array_to_memory(&mut caller, &pads)
        },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuSurfaceGamepadConnected",
        |id: i32, pad: i32| -> i32 { i32::from(surface::gamepad_connected(id, pad)) },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuSurfaceGamepadButtonDown",
        |id: i32, pad: i32, button: i32| -> i32 {
            i32::from(surface::gamepad_button_down(id, pad, button))
        },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuSurfaceGamepadAxis",
        |id: i32, pad: i32, axis: i32| -> f32 { surface::gamepad_axis(id, pad, axis) },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuSurfaceFocused",
        |id: i32| -> i32 { i32::from(surface::focused(id)) },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuSurfaceCloseRequested",
        |id: i32| -> i32 { i32::from(surface::close_requested(id)) },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuSurfacePollEvents",
        |mut caller: Caller<'_, ()>, id: i32| -> Result<i32> {
            let bytes = surface::poll_events_bytes(id);
            super::memory::write_bytes_to_memory(&mut caller, &bytes)
        },
    )?;
    linker.func_wrap("Dream", "gpuSurfaceWidth", |id: i32| -> i32 {
        surface::width(id)
    })?;
    linker.func_wrap("Dream", "gpuSurfaceHeight", |id: i32| -> i32 {
        surface::height(id)
    })?;
    linker.func_wrap(
        "Dream",
        "gpuRenderBlit",
        |mut caller: Caller<'_, ()>, sid: i32, tid: i32| -> Result<i32> {
            let code = surface::blit(sid, tid);
            resolve_host_future_i32(&mut caller, code)
        },
    )?;

    linker.func_wrap(
        "Dream",
        "gpuRenderPipelineCreate",
        |mut caller: Caller<'_, ()>, vs: i32, fs: i32| -> Result<i32> {
            let vs_name = read_arg_string(&mut caller, vs)?;
            let fs_name = read_arg_string(&mut caller, fs)?;
            let id = render::pipeline_create_ex(&vs_name, &fs_name, 0, 0, 0, 0, 0, 0, 0, 1);
            resolve_host_future_i32(&mut caller, id)
        },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuRenderPipelineCreateEx",
        |mut caller: Caller<'_, ()>,
         vs: i32,
         fs: i32,
         topology: i32,
         cull: i32,
         ff: i32,
         de: i32,
         dw: i32,
         dc: i32,
         be: i32,
         sc: i32|
         -> Result<i32> {
            let vs_name = read_arg_string(&mut caller, vs)?;
            let fs_name = read_arg_string(&mut caller, fs)?;
            let id = render::pipeline_create_ex(
                &vs_name, &fs_name, topology, cull, ff, de, dw, dc, be, sc,
            );
            resolve_host_future_i32(&mut caller, id)
        },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuRenderDraw",
        |mut caller: Caller<'_, ()>,
         sid: i32,
         pid: i32,
         vb: i32,
         n: i32,
         uniforms: i32,
         cr: f32,
         cg: f32,
         cb: f32,
         ca: f32|
         -> Result<i32> {
            let u = read_arg_bytes(&mut caller, uniforms)?;
            let code = render::draw_ex(sid, pid, vb, n, 1, &u, [cr, cg, cb, ca], -1, 0, None);
            resolve_host_future_i32(&mut caller, code)
        },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuRenderDrawEx",
        |mut caller: Caller<'_, ()>,
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
         load: i32|
         -> Result<i32> {
            let u = read_arg_bytes(&mut caller, uniforms)?;
            let code =
                render::draw_ex(sid, pid, vb, n, inst, &u, [cr, cg, cb, ca], depth, load, None);
            resolve_host_future_i32(&mut caller, code)
        },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuRenderDrawIndexed",
        |mut caller: Caller<'_, ()>,
         sid: i32,
         pid: i32,
         vb: i32,
         ib: i32,
         n: i32,
         uniforms: i32,
         cr: f32,
         cg: f32,
         cb: f32,
         ca: f32|
         -> Result<i32> {
            let u = read_arg_bytes(&mut caller, uniforms)?;
            let code =
                render::draw_ex(sid, pid, vb, 0, 1, &u, [cr, cg, cb, ca], -1, 0, Some((ib, n)));
            resolve_host_future_i32(&mut caller, code)
        },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuRenderDrawIndexedEx",
        |mut caller: Caller<'_, ()>,
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
         load: i32|
         -> Result<i32> {
            let u = read_arg_bytes(&mut caller, uniforms)?;
            let code = render::draw_ex(
                sid,
                pid,
                vb,
                0,
                inst,
                &u,
                [cr, cg, cb, ca],
                depth,
                load,
                Some((ib, n)),
            );
            resolve_host_future_i32(&mut caller, code)
        },
    )?;

    linker.func_wrap("Dream", "gpuPassBegin", || -> i32 { compute::pass_begin() })?;
    linker.func_wrap(
        "Dream",
        "gpuPassDispatch",
        |mut caller: Caller<'_, ()>,
         pass: i32,
         kernel_ptr: i32,
         bufs: i32,
         tex: i32,
         samp: i32,
         ex: i32,
         ey: i32,
         ez: i32,
         uniforms_ptr: i32| {
            let kernel = read_arg_string(&mut caller, kernel_ptr)?;
            let buffer_ids = read_arg_i32_array(&mut caller, bufs)?;
            let texture_ids = read_arg_i32_array(&mut caller, tex)?;
            let sampler_ids = read_arg_i32_array(&mut caller, samp)?;
            let uniforms = read_arg_bytes(&mut caller, uniforms_ptr)?;
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
            Ok(())
        },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuPassDispatchIndirect",
        |mut caller: Caller<'_, ()>,
         pass: i32,
         kernel_ptr: i32,
         bufs: i32,
         tex: i32,
         samp: i32,
         indirect: i32,
         off: i32| {
            let kernel = read_arg_string(&mut caller, kernel_ptr)?;
            let buffer_ids = read_arg_i32_array(&mut caller, bufs)?;
            let texture_ids = read_arg_i32_array(&mut caller, tex)?;
            let sampler_ids = read_arg_i32_array(&mut caller, samp)?;
            compute::pass_dispatch_indirect(
                pass,
                kernel,
                buffer_ids,
                texture_ids,
                sampler_ids,
                indirect,
                off,
            );
            Ok(())
        },
    )?;
    linker.func_wrap(
        "Dream",
        "gpuPassSubmit",
        |mut caller: Caller<'_, ()>, pass_id: i32| -> Result<i32> {
            let code = compute::pass_submit(pass_id);
            resolve_host_future_i32(&mut caller, code)
        },
    )?;

    // delayMs for Time.delay
    linker.func_wrap(
        "Dream",
        "delayMs",
        |mut caller: Caller<'_, ()>, ms: i32| -> Result<i32> {
            if ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(ms as u64));
            }
            resolve_host_future_void(&mut caller)
        },
    )?;

    Ok(())
}

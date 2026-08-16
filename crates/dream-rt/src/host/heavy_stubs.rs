//! Linkable stand-ins when `http` / `net` / `gpu` / `webview` features are off.
//! Keeps clang happy without compiling reqwest, wgpu, or wry.

#![allow(unused_variables)]


#[cfg(not(feature = "http"))]
mod http {
    use crate::guest;

    fn err() -> i32 {
        guest::write_bytes(b"error\nunavailable")
    }

    #[no_mangle]
    pub extern "C" fn dream_http_request(
        url: i32,
        method: i32,
        headers: i32,
        body: i32,
        timeout_ms: i32,
        http_version: i32,
    ) -> i32 {
        err()
    }

    #[no_mangle]
    pub extern "C" fn dream_http_request_bytes(
        url: i32,
        method: i32,
        headers: i32,
        body: i32,
        timeout_ms: i32,
        http_version: i32,
    ) -> i32 {
        err()
    }

    #[no_mangle]
    pub extern "C" fn dream_http_request_stream(
        url: i32,
        method: i32,
        headers: i32,
        body: i32,
        timeout_ms: i32,
        http_version: i32,
    ) -> i32 {
        err()
    }

    #[no_mangle]
    pub extern "C" fn dream_http_request_stream_bytes(
        url: i32,
        method: i32,
        headers: i32,
        body: i32,
        timeout_ms: i32,
        http_version: i32,
    ) -> i32 {
        err()
    }

    #[no_mangle]
    pub extern "C" fn dream_http_read_chunk(handle: i32, max_bytes: i32) -> i32 {
        err()
    }

    #[no_mangle]
    pub extern "C" fn dream_http_close_stream(handle: i32) -> i32 {
        0
    }
}

#[cfg(not(feature = "net"))]
mod net {
    use crate::guest;

    fn err() -> i32 {
        guest::write_bytes(b"ECONNECT")
    }

    #[no_mangle]
    pub extern "C" fn dream_tcp_connect(host: i32, port: i32, timeout_ms: i32) -> i32 {
        err()
    }
    #[no_mangle]
    pub extern "C" fn dream_tcp_send(handle: i32, data: i32) -> i32 {
        err()
    }
    #[no_mangle]
    pub extern "C" fn dream_tcp_send_text(handle: i32, text: i32) -> i32 {
        err()
    }
    #[no_mangle]
    pub extern "C" fn dream_tcp_receive(handle: i32, max_bytes: i32) -> i32 {
        err()
    }
    #[no_mangle]
    pub extern "C" fn dream_tcp_close(handle: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_ws_connect(url: i32, timeout_ms: i32) -> i32 {
        err()
    }
    #[no_mangle]
    pub extern "C" fn dream_ws_send_text(handle: i32, text: i32) -> i32 {
        err()
    }
    #[no_mangle]
    pub extern "C" fn dream_ws_send_binary(handle: i32, data: i32) -> i32 {
        err()
    }
    #[no_mangle]
    pub extern "C" fn dream_ws_receive(handle: i32) -> i32 {
        err()
    }
    #[no_mangle]
    pub extern "C" fn dream_ws_close(handle: i32, code: i32, reason: i32) -> i32 {
        0
    }
}

#[cfg(not(feature = "gpu"))]
mod gpu {
    use crate::guest;

    #[no_mangle]
    pub extern "C" fn dream_gpu_is_available() -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_ready() -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_last_error() -> i32 {
        guest::intern("unavailable")
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_try_init() -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_frame() -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_timestamp() -> i64 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_buffer_alloc_bytes(n: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_buffer_alloc_vertex_bytes(n: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_buffer_write_bytes(id: i32, data_ptr: i32) {}
    #[no_mangle]
    pub extern "C" fn dream_gpu_buffer_write_bytes_at(id: i32, byte_offset: i32, data_ptr: i32) {}
    #[no_mangle]
    pub extern "C" fn dream_gpu_buffer_read_bytes(id: i32, n: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_buffer_read_bytes_at(id: i32, byte_offset: i32, n: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_buffer_copy(
        src_id: i32,
        dst_id: i32,
        src_off: i32,
        dst_off: i32,
        size: i32,
    ) {
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_buffer_destroy(id: i32) {}
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
        0
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
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_shader_from_wgsl(source_ptr: i32, entry_ptr: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_dispatch_shader(
        shader_id: i32,
        bufs: i32,
        wx: i32,
        wy: i32,
        wz: i32,
    ) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_sampler_create(filter: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_sampler_create_ex(filter: i32, address: i32, mip: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_texture_create_rgba8(w: i32, h: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_texture_create_depth(w: i32, h: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_texture_create_rgba16_float(w: i32, h: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_texture_create_cube_rgba8(size: i32) -> i32 {
        0
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
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_texture_read_rgba(id: i32) -> i32 {
        0
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
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_texture_generate_mipmaps(id: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_texture_destroy(id: i32) {}
    #[no_mangle]
    pub extern "C" fn dream_gpu_sampler_destroy(id: i32) {}
    #[no_mangle]
    pub extern "C" fn dream_gpu_surface_from_canvas(id_ptr: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_surface_create(title_ptr: i32, width: i32, height: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_surface_configure(id: i32, w: i32, h: i32) {}
    #[no_mangle]
    pub extern "C" fn dream_gpu_surface_present(id: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_surface_pointer(id: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_surface_mods(id: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_surface_key_down(id: i32, code_ptr: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_surface_gamepads(id: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_surface_gamepad_connected(id: i32, pad: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_surface_gamepad_button_down(id: i32, pad: i32, button: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_surface_gamepad_axis(id: i32, pad: i32, axis: i32) -> f32 {
        0.0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_surface_focused(id: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_surface_close_requested(id: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_surface_poll_events(id: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_surface_width(id: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_surface_height(id: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_render_blit(sid: i32, tid: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_render_pipeline_create(vs: i32, fs: i32) -> i32 {
        0
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
        0
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
        0
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
        0
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
        0
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
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_render_pipeline_destroy(id: i32) {}
    #[no_mangle]
    pub extern "C" fn dream_gpu_surface_destroy(id: i32) {}
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
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_pass_begin() -> i32 {
        0
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
    }
    #[no_mangle]
    pub extern "C" fn dream_gpu_pass_submit(pass_id: i32) -> i32 {
        0
    }
}

#[cfg(not(feature = "webview"))]
mod webview {
    use crate::guest;

    #[no_mangle]
    pub extern "C" fn dream_webview_create(title: i32, width: i32, height: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_webview_load_url(id: i32, url: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_webview_load_html(id: i32, html: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_webview_load_file(id: i32, path: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_webview_close(id: i32) {}
    #[no_mangle]
    pub extern "C" fn dream_webview_close_requested(id: i32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn dream_webview_tick(id: i32) -> i32 {
        guest::write_bytes(&[])
    }
    #[no_mangle]
    pub extern "C" fn dream_webview_poll(id: i32) -> i32 {
        guest::write_bytes(&[])
    }
    #[no_mangle]
    pub extern "C" fn dream_webview_reply(id: i32, reply_id: i32, body: i32) {}
    #[no_mangle]
    pub extern "C" fn dream_webview_reply_err(id: i32, reply_id: i32, message: i32) {}
    #[no_mangle]
    pub extern "C" fn dream_webview_reply_bytes(id: i32, reply_id: i32, body: i32) {}
    #[no_mangle]
    pub extern "C" fn dream_webview_emit(id: i32, channel: i32, body: i32) {}
    #[no_mangle]
    pub extern "C" fn dream_webview_emit_bytes(id: i32, channel: i32, body: i32) {}
    #[no_mangle]
    pub extern "C" fn dream_webview_eval(id: i32, js: i32) -> i32 {
        guest::write_bytes(&[])
    }
}

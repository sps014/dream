//! C ABI for the native-C guest: GPU/time/print helpers live in the `dream` cdylib
//! and are resolved when `cc` links the generated program with `-ldream`.
//!
//! Pointer arguments are guest heap addresses from the C runtime, not Rust references.
#![allow(clippy::missing_safety_doc)]

use crate::execution::host::gpu::{
    attach_abi_from_wat_path, buffers, compute, device, error, render, surface, textures,
};
use std::sync::{Mutex, Once};

type AllocFn = unsafe extern "C" fn(i32) -> usize;
type ArrayNewFn = unsafe extern "C" fn(i32, i32) -> usize;

struct GuestAlloc {
    string_alloc: Option<AllocFn>,
    array_new: Option<ArrayNewFn>,
}

static GUEST: Mutex<GuestAlloc> = Mutex::new(GuestAlloc {
    string_alloc: None,
    array_new: None,
});

#[no_mangle]
pub extern "C" fn dream_host_bind(string_alloc: AllocFn, array_new: ArrayNewFn) {
    let mut g = GUEST.lock().expect("guest alloc");
    g.string_alloc = Some(string_alloc);
    g.array_new = Some(array_new);
}

pub fn attach_gpu_abi_beside(path: &str) {
    attach_abi_from_wat_path(path);
}

fn ensure_abi() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        if let Ok(p) = std::env::var("DREAM_NATIVE_C") {
            attach_abi_from_wat_path(&p);
        }
    });
}

unsafe fn read_string(p: usize) -> String {
    if p == 0 {
        return String::new();
    }
    let n = *(p as *const i32);
    if n <= 0 {
        return String::new();
    }
    const DREAM_STR_SLICE: i32 = dream_mir::abi::DREAM_STR_SLICE;
    let pad = *((p as *const i32).add(1));
    let units = if pad == DREAM_STR_SLICE {
        let d = std::ptr::read(
            (p as *const u8)
                .add(dream_mir::abi::STRING_HEADER_SIZE as usize + std::mem::size_of::<usize>())
                .cast::<*const u16>(),
        );
        std::slice::from_raw_parts(d, n as usize)
    } else {
        std::slice::from_raw_parts(
            (p as *const u8)
                .add(dream_mir::abi::STRING_UNITS_OFFSET as usize)
                .cast::<u16>(),
            n as usize,
        )
    };
    String::from_utf16_lossy(units)
}

unsafe fn read_bytes(p: usize) -> Vec<u8> {
    if p == 0 {
        return Vec::new();
    }
    let n = *(p as *const i32);
    if n <= 0 {
        return Vec::new();
    }
    std::slice::from_raw_parts((p as *const u8).add(4), n as usize).to_vec()
}

unsafe fn read_i32s(p: usize) -> Vec<i32> {
    if p == 0 {
        return Vec::new();
    }
    let n = *(p as *const i32);
    if n <= 0 {
        return Vec::new();
    }
    std::slice::from_raw_parts((p as *const u8).add(4).cast::<i32>(), n as usize).to_vec()
}

fn alloc_string(s: &str) -> usize {
    let units: Vec<u16> = s.encode_utf16().collect();
    let alloc = GUEST.lock().ok().and_then(|g| g.string_alloc);
    let Some(alloc) = alloc else {
        return 0;
    };
    unsafe {
        let p = alloc(units.len() as i32);
        if p == 0 {
            return 0;
        }
        let dst = (p as *mut u8)
            .add(dream_mir::abi::STRING_UNITS_OFFSET as usize)
            .cast::<u16>();
        std::ptr::copy_nonoverlapping(units.as_ptr(), dst, units.len());
        p
    }
}

fn alloc_bytes(bytes: &[u8]) -> usize {
    let alloc = GUEST.lock().ok().and_then(|g| g.array_new);
    let Some(alloc) = alloc else {
        return 0;
    };
    unsafe {
        let p = alloc(bytes.len() as i32, 1);
        if p == 0 {
            return 0;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), (p as *mut u8).add(4), bytes.len());
        p
    }
}

fn alloc_i32s(xs: &[i32]) -> usize {
    let alloc = GUEST.lock().ok().and_then(|g| g.array_new);
    let Some(alloc) = alloc else {
        return 0;
    };
    unsafe {
        let p = alloc(xs.len() as i32, 4);
        if p == 0 {
            return 0;
        }
        std::ptr::copy_nonoverlapping(xs.as_ptr(), (p as *mut u8).add(4).cast::<i32>(), xs.len());
        p
    }
}

#[no_mangle]
pub extern "C" fn gpuIsAvailable() -> i32 {
    i32::from(device::is_available())
}

#[no_mangle]
pub extern "C" fn gpuReady() -> i32 {
    i32::from(crate::execution::host::gpu::is_ready())
}

#[no_mangle]
pub extern "C" fn gpuLastError() -> usize {
    alloc_string(&error::take_last_error())
}

#[no_mangle]
pub extern "C" fn gpuTryInit() -> i32 {
    ensure_abi();
    device::try_init()
}

#[no_mangle]
pub extern "C" fn gpuFrame() -> i32 {
    surface::wait_display_frame();
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
pub extern "C" fn gpuSurfaceConfigure(id: i32, w: i32, h: i32) {
    surface::configure(id, w, h);
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
pub unsafe extern "C" fn gpuRenderDrawEx(
    sid: i32,
    pid: i32,
    vb: i32,
    n: i32,
    inst: i32,
    uniforms: usize,
    cr: f32,
    cg: f32,
    cb: f32,
    ca: f32,
    depth: i32,
    load: i32,
) -> i32 {
    render::draw_ex(
        sid,
        pid,
        vb,
        n,
        inst,
        &read_bytes(uniforms),
        [cr, cg, cb, ca],
        depth,
        load,
        None,
    )
}

#[no_mangle]
pub unsafe extern "C" fn gpuRenderDraw(
    sid: i32,
    pid: i32,
    vb: i32,
    n: i32,
    uniforms: usize,
    cr: f32,
    cg: f32,
    cb: f32,
    ca: f32,
) -> i32 {
    render::draw_ex(
        sid,
        pid,
        vb,
        n,
        1,
        &read_bytes(uniforms),
        [cr, cg, cb, ca],
        -1,
        0,
        None,
    )
}

#[no_mangle]
pub extern "C" fn gpuRenderPipelineDestroy(id: i32) {
    render::pipeline_destroy(id);
}

#[no_mangle]
pub extern "C" fn gpuSamplerCreate(filter: i32) -> i32 {
    textures::sampler_create(filter)
}

#[no_mangle]
pub extern "C" fn gpuSamplerCreateEx(filter: i32, address: i32, mip_filter: i32) -> i32 {
    textures::sampler_create_ex(filter, address, mip_filter)
}

#[no_mangle]
pub extern "C" fn gpuTextureCreateRgba8(w: i32, h: i32) -> i32 {
    textures::texture_create_rgba8(w, h)
}

#[no_mangle]
pub extern "C" fn gpuTextureCreateDepth(w: i32, h: i32) -> i32 {
    textures::texture_create_depth(w, h)
}

#[no_mangle]
pub extern "C" fn gpuTextureCreateRgba16Float(w: i32, h: i32) -> i32 {
    textures::texture_create_rgba16float(w, h)
}

#[no_mangle]
pub extern "C" fn gpuTextureCreateCubeRgba8(size: i32) -> i32 {
    textures::texture_create_cube_rgba8(size)
}

#[no_mangle]
pub extern "C" fn gpuTextureDestroy(id: i32) {
    textures::texture_destroy(id);
}

#[no_mangle]
pub extern "C" fn gpuPassBegin() -> i32 {
    compute::pass_begin()
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
pub unsafe extern "C" fn gpuSurfaceFromCanvas(id: usize) -> i32 {
    surface::from_canvas(&read_string(id))
}

#[no_mangle]
pub extern "C" fn gpuSurfacePointer(id: i32) -> usize {
    alloc_bytes(&surface::pointer_bytes(id))
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
pub unsafe extern "C" fn unicodeNormalize(text: usize, form: i32) -> usize {
    use unicode_normalization::UnicodeNormalization;
    let s = read_string(text);
    let out = match form {
        1 => s.nfd().collect::<String>(),
        2 => s.nfkc().collect::<String>(),
        3 => s.nfkd().collect::<String>(),
        _ => s.nfc().collect::<String>(),
    };
    alloc_string(&out)
}

#[no_mangle]
pub unsafe extern "C" fn unicodeToLower(text: usize) -> usize {
    alloc_string(&read_string(text).to_lowercase())
}

#[no_mangle]
pub unsafe extern "C" fn unicodeToUpper(text: usize) -> usize {
    alloc_string(&read_string(text).to_uppercase())
}

#[no_mangle]
pub unsafe extern "C" fn unicodeGraphemes(text: usize) -> usize {
    use unicode_segmentation::UnicodeSegmentation;
    let s = read_string(text);
    let parts: Vec<String> = s.graphemes(true).map(str::to_string).collect();
    alloc_string_array(&parts)
}

fn alloc_string_array(items: &[String]) -> usize {
    let alloc = GUEST.lock().ok().and_then(|g| g.array_new);
    let Some(alloc) = alloc else {
        return 0;
    };
    unsafe {
        let p = alloc(items.len() as i32, 8);
        if p == 0 {
            return 0;
        }
        let slots = (p as *mut u8).add(4).cast::<usize>();
        for (i, item) in items.iter().enumerate() {
            slots.add(i).write(alloc_string(item));
        }
        p
    }
}

#[no_mangle]
pub unsafe extern "C" fn cryptoAesGcmEncrypt(
    key: usize,
    nonce: usize,
    plaintext: usize,
    aad: usize,
) -> usize {
    use aes_gcm::aead::{Aead, KeyInit, Payload};
    use aes_gcm::{Aes256Gcm, Nonce};
    let key = read_bytes(key);
    let nonce_bytes = read_bytes(nonce);
    let plaintext = read_bytes(plaintext);
    let aad = read_bytes(aad);
    let Ok(cipher) = Aes256Gcm::new_from_slice(&key) else {
        return alloc_bytes(&[]);
    };
    if nonce_bytes.len() != 12 {
        return alloc_bytes(&[]);
    }
    let nonce = Nonce::from_slice(&nonce_bytes);
    match cipher.encrypt(
        nonce,
        Payload {
            msg: &plaintext,
            aad: &aad,
        },
    ) {
        Ok(out) => alloc_bytes(&out),
        Err(_) => alloc_bytes(&[]),
    }
}

#[no_mangle]
pub unsafe extern "C" fn cryptoAesGcmDecrypt(
    key: usize,
    nonce: usize,
    ciphertext: usize,
    aad: usize,
) -> usize {
    use aes_gcm::aead::{Aead, KeyInit, Payload};
    use aes_gcm::{Aes256Gcm, Nonce};
    let key = read_bytes(key);
    let nonce_bytes = read_bytes(nonce);
    let ciphertext = read_bytes(ciphertext);
    let aad = read_bytes(aad);
    let Ok(cipher) = Aes256Gcm::new_from_slice(&key) else {
        return alloc_bytes(&[0u8]);
    };
    if nonce_bytes.len() != 12 {
        return alloc_bytes(&[0u8]);
    }
    let nonce = Nonce::from_slice(&nonce_bytes);
    match cipher.decrypt(
        nonce,
        Payload {
            msg: &ciphertext,
            aad: &aad,
        },
    ) {
        Ok(plain) => {
            let mut tagged = Vec::with_capacity(1 + plain.len());
            tagged.push(1u8);
            tagged.extend_from_slice(&plain);
            alloc_bytes(&tagged)
        }
        Err(_) => alloc_bytes(&[0u8]),
    }
}

#[no_mangle]
pub unsafe extern "C" fn httpRequest(
    url: usize,
    method: usize,
    headers: usize,
    body: usize,
    timeout_ms: i32,
    http_version: i32,
) -> usize {
    let out = crate::execution::host::http::perform_http(
        &read_string(method),
        &read_string(url),
        &read_string(headers),
        read_string(body).into_bytes(),
        timeout_ms,
        http_version,
    );
    alloc_bytes(&out)
}

#[no_mangle]
pub unsafe extern "C" fn httpRequestBytes(
    url: usize,
    method: usize,
    headers: usize,
    body: usize,
    timeout_ms: i32,
    http_version: i32,
) -> usize {
    let out = crate::execution::host::http::perform_http(
        &read_string(method),
        &read_string(url),
        &read_string(headers),
        read_bytes(body),
        timeout_ms,
        http_version,
    );
    alloc_bytes(&out)
}

#[no_mangle]
pub unsafe extern "C" fn httpRequestStream(
    url: usize,
    method: usize,
    headers: usize,
    body: usize,
    timeout_ms: i32,
    http_version: i32,
) -> usize {
    let out = crate::execution::host::http::open_http_stream(
        &read_string(method),
        &read_string(url),
        &read_string(headers),
        read_string(body).into_bytes(),
        timeout_ms,
        http_version,
    );
    alloc_bytes(&out)
}

#[no_mangle]
pub unsafe extern "C" fn httpRequestStreamBytes(
    url: usize,
    method: usize,
    headers: usize,
    body: usize,
    timeout_ms: i32,
    http_version: i32,
) -> usize {
    let out = crate::execution::host::http::open_http_stream(
        &read_string(method),
        &read_string(url),
        &read_string(headers),
        read_bytes(body),
        timeout_ms,
        http_version,
    );
    alloc_bytes(&out)
}

#[no_mangle]
pub extern "C" fn httpReadChunk(handle: i32, max_bytes: i32) -> usize {
    alloc_bytes(&crate::execution::host::http::http_read_chunk(
        handle, max_bytes,
    ))
}

#[no_mangle]
pub extern "C" fn httpCloseStream(handle: i32) -> i32 {
    crate::execution::host::http::http_close_stream(handle)
}

//! LLVM names, types, and runtime declarations.

use dream_mir::{Callee, Const, Mir, MirFunction, Operand, Place, Rvalue};
use dream_types::{PrimTy, TyKind, TypeId, TypeInterner};
use std::collections::HashMap;

pub(crate) fn dbg_spill_count(func: &MirFunction, interner: &TypeInterner) -> u32 {
    func.locals
        .iter()
        .filter(|d| {
            d.name
                .as_deref()
                .is_some_and(|n| !n.starts_with("__"))
                && !matches!(interner.kind(d.ty), TyKind::Void | TyKind::Error)
        })
        .count() as u32
}

pub(crate) fn debug_file_id(mir: &Mir, func: &MirFunction) -> i32 {
    let Some(path) = &func.file else {
        return 0;
    };
    let mut id = 0i32;
    let mut seen: Vec<&str> = Vec::new();
    for f in &mir.functions {
        let Some(p) = f.file.as_deref() else {
            continue;
        };
        if seen.contains(&p) {
            continue;
        }
        if p == path {
            return id;
        }
        seen.push(p);
        id += 1;
    }
    0
}

pub(crate) fn take_move_src(
    func: &MirFunction,
    interner: &TypeInterner,
    rv: &Rvalue,
) -> Option<dream_mir::Local> {
    match rv {
        Rvalue::Use(Operand::Copy(Place::Local(l))) => {
            let d = func.locals.get(l.0 as usize)?;
            if d.is_take && interner.is_rc_tracked(d.ty) {
                Some(*l)
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(crate) fn retain_on_store(func: &MirFunction, rv: &Rvalue) -> bool {
    match rv {
        Rvalue::Use(Operand::Const(Const::Str(_))) => true,
        Rvalue::Use(Operand::Copy(Place::Local(l))) => func
            .locals
            .get(l.0 as usize)
            .map(|d| !d.is_take)
            .unwrap_or(true),
        Rvalue::Use(Operand::Copy(_)) => true,
        _ => false,
    }
}

pub(crate) fn format_phi(inc: &[(String, String)]) -> String {
    inc.iter()
        .map(|(v, l)| format!("[ {}, %{} ]", v, l))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn fmt_sym(kind: &str, ty: TypeId) -> String {
    llvm_fn_name(&format!("{}_{}", kind, ty.0))
}

pub(crate) fn llvm_fp_hex(v: f64) -> String {
    format!("0x{:016X}", v.to_bits())
}

pub(crate) fn runtime_tag(interner: &TypeInterner, ty: TypeId, tags: &HashMap<TypeId, i32>) -> i32 {
    match interner.kind(ty) {
        TyKind::Prim(PrimTy::Int) => dream_mir::abi::TAG_INT,
        TyKind::Prim(PrimTy::Float) => dream_mir::abi::TAG_FLOAT,
        TyKind::Prim(PrimTy::Double) => dream_mir::abi::TAG_DOUBLE,
        TyKind::Prim(PrimTy::Bool) => dream_mir::abi::TAG_BOOL,
        TyKind::Prim(PrimTy::String) => dream_mir::abi::TAG_STRING,
        TyKind::Array(_) => dream_mir::abi::TAG_ARRAY,
        TyKind::Prim(PrimTy::Char) => dream_mir::abi::TAG_CHAR,
        TyKind::Prim(PrimTy::Long) => dream_mir::abi::TAG_LONG,
        TyKind::Prim(PrimTy::UInt) => dream_mir::abi::TAG_UINT,
        TyKind::Prim(PrimTy::ULong) => dream_mir::abi::TAG_ULONG,
        TyKind::Prim(PrimTy::Byte) => dream_mir::abi::TAG_BYTE,
        _ => tags.get(&ty).copied().unwrap_or(dream_mir::abi::TAG_STRUCT_BASE),
    }
}

pub(crate) fn llvm_fn_ret(interner: &TypeInterner, layouts: &dream_hir::LayoutTable, func: &MirFunction) -> TypeId {
    if func.is_async {
        unwrap_future(interner, layouts, func.ret)
    } else {
        func.ret
    }
}

pub(crate) fn llvm_fn_ret_callee(interner: &TypeInterner, mir: &Mir, callee: &Callee) -> TypeId {
    unwrap_future(interner, &mir.layouts, callee.ret)
}

pub(crate) fn unwrap_future(interner: &TypeInterner, layouts: &dream_hir::LayoutTable, ty: TypeId) -> TypeId {
    let is_fut = layouts
        .get(ty)
        .is_some_and(|l| l.name == "Future" || l.name.starts_with("Future<"));
    if is_fut {
        if let TyKind::Struct(_, args) = interner.kind(ty) {
            if let Some(inner) = args.first() {
                return *inner;
            }
        }
    }
    ty
}

pub(crate) fn resolved_symbol(sym: &str) -> String {
    if let Some(c) = native_c_sym(sym) {
        c.to_string()
    } else {
        llvm_fn_name(sym)
    }
}

pub(crate) fn retain_sym(interner: &TypeInterner, ty: TypeId) -> &'static str {
    if interner.is_shared_type(ty) {
        "dream_retain_shared"
    } else {
        "dream_retain"
    }
}

pub(crate) fn release_sym(interner: &TypeInterner, ty: TypeId) -> &'static str {
    if interner.is_shared_type(ty) {
        "dream_release_shared"
    } else {
        "dream_release"
    }
}

pub(crate) fn llvm_extern_name(key: &str) -> String {
    if let Some(c) = native_c_sym(key) {
        c.to_string()
    } else if key == "sleep" || key.ends_with("sleep") {
        "d_sleep".into()
    } else {
        llvm_fn_name(key)
    }
}

/// Maps `@intrinsic` runtime helpers and `@js` host fields that have a `dream-rt`
/// implementation. `js*` (browser/Node) stay `None` — native is a compile-time error.
pub(crate) fn native_c_sym(key: &str) -> Option<&'static str> {
    let k = key.rsplit(['.', ':']).next().unwrap_or(key);
    let k = k.strip_suffix("_host").unwrap_or(k);
    match k {
        "funcbox_new" => Some("d_funcbox_new"),
        "funcbox_funcidx" => Some("d_funcbox_funcidx"),
        "funcbox_env" => Some("d_funcbox_env"),
        "string_alloc" => Some("dream_string_alloc"),
        "string_from_utf8" => Some("dream_string_from_utf8"),
        "string_from_utf8_prefix" => Some("dream_string_from_utf8_prefix"),
        "string_copy_utf8" => Some("dream_string_copy_utf8"),
        "string_clone" => Some("dream_string_clone"),
        "string_compare" => Some("dream_string_compare"),
        "string_substring_raw" => Some("dream_string_substring_raw"),
        "string_set" => Some("dream_string_set"),
        "utf8_decode_at" => Some("dream_utf8_decode_at"),
        "utf8_width_at" => Some("dream_utf8_width_at"),
        "simd_f32x4_add" => Some("dream_simd_f32x4_add"),
        "simd_f32x4_sub" => Some("dream_simd_f32x4_sub"),
        "simd_f32x4_mul" => Some("dream_simd_f32x4_mul"),
        "simd_i32x4_add" => Some("dream_simd_i32x4_add"),
        "nano_time" | "timeNowNanos" => Some("dream_nano_time"),
        "now_millis" | "dateNowMillis" => Some("dream_now_millis"),
        "dateLocalOffsetMinutes" => Some("dream_date_local_offset_minutes"),
        "dateZoneOffsetMinutes" => Some("dream_date_zone_offset_minutes"),
        "dateLocalZoneName" => Some("dream_date_local_zone_name"),
        "abs" => Some("dream_math_abs"),
        "floor" => Some("dream_math_floor"),
        "ceil" => Some("dream_math_ceil"),
        "round" => Some("dream_math_round"),
        "sqrt" => Some("dream_math_sqrt"),
        "pow" => Some("dream_math_pow"),
        "sin" => Some("dream_math_sin"),
        "cos" => Some("dream_math_cos"),
        "tan" => Some("dream_math_tan"),
        "asin" => Some("dream_math_asin"),
        "acos" => Some("dream_math_acos"),
        "atan" => Some("dream_math_atan"),
        "atan2" => Some("dream_math_atan2"),
        "file_read" | "fileRead" => Some("dream_file_read"),
        "file_write" | "fileWrite" => Some("dream_file_write"),
        "file_append" | "fileAppend" => Some("dream_file_append"),
        "file_read_bytes" | "fileReadBytes" => Some("dream_file_read_bytes"),
        "file_write_bytes" | "fileWriteBytes" => Some("dream_file_write_bytes"),
        "file_exists" | "fileExists" => Some("dream_file_exists"),
        "file_delete" | "fileDelete" => Some("dream_file_delete"),
        "file_size" | "fileSize" => Some("dream_file_size"),
        "file_is_dir" | "fileIsDir" => Some("dream_file_is_dir"),
        "dir_list" | "dirList" => Some("dream_dir_list"),
        "dir_create" | "dirCreate" => Some("dream_dir_create"),
        "dir_create_all" | "dirCreateAll" => Some("dream_dir_create_all"),
        "processPlatform" => Some("dream_process_platform"),
        "processOsFamily" => Some("dream_process_os_family"),
        "processCwd" => Some("dream_process_cwd"),
        "processSetCwd" => Some("dream_process_set_cwd"),
        "processArgs" => Some("dream_process_args"),
        "processEnvGet" => Some("dream_process_env_get"),
        "processEnvSet" => Some("dream_process_env_set"),
        "processExePath" => Some("dream_process_exe_path"),
        "consoleExit" => Some("dream_console_exit"),
        "consoleReadLine" => Some("dream_console_read_line"),
        "consoleReadKey" => Some("dream_console_read_key"),
        "delayMs" => Some("dream_delay_ms"),
        "shared_lock_acquire" => Some("dream_lock_acquire"),
        "shared_lock_release" => Some("dream_lock_release"),
        "shared_lock_try_acquire" => Some("dream_lock_try_acquire"),
        "shared_lock_try_acquire_for" => Some("dream_lock_try_acquire_for"),
        "shared_semaphore_acquire" => Some("dream_sem_acquire"),
        "shared_semaphore_release" => Some("dream_sem_release"),
        "shared_semaphore_try_acquire" => Some("dream_sem_try_acquire"),
        "shared_semaphore_try_acquire_for" => Some("dream_sem_try_acquire_for"),
        "dream_cancel" => Some("dream_cancel"),
        "fileOpen" => Some("dream_file_open"),
        "fileHandleRead" => Some("dream_file_handle_read"),
        "fileHandleWrite" => Some("dream_file_handle_write"),
        "fileHandleSeek" => Some("dream_file_handle_seek"),
        "fileHandleClose" => Some("dream_file_handle_close"),
        "processRun" => Some("dream_process_run"),
        "processSpawn" => Some("dream_process_spawn"),
        "processWriteStdin" => Some("dream_process_write_stdin"),
        "processReadStream" => Some("dream_process_read_stream"),
        "processReadStreamLine" => Some("dream_process_read_stream_line"),
        "processWait" => Some("dream_process_wait"),
        "processKill" => Some("dream_process_kill"),
        "cryptoSha256" => Some("dream_crypto_sha256"),
        "cryptoSha512" => Some("dream_crypto_sha512"),
        "cryptoHmacSha256" => Some("dream_crypto_hmac_sha256"),
        "cryptoSecureRandomBytes" => Some("dream_crypto_secure_random_bytes"),
        "cryptoSecureRandomFill" => Some("dream_crypto_secure_random_fill"),
        "cryptoAesGcmEncrypt" => Some("dream_crypto_aes_gcm_encrypt"),
        "cryptoAesGcmDecrypt" => Some("dream_crypto_aes_gcm_decrypt"),
        "unicodeNormalize" => Some("dream_unicode_normalize"),
        "unicodeToLower" => Some("dream_unicode_to_lower"),
        "unicodeToUpper" => Some("dream_unicode_to_upper"),
        "unicodeGraphemes" => Some("dream_unicode_graphemes"),
        "httpRequest" => Some("dream_http_request"),
        "httpRequestBytes" => Some("dream_http_request_bytes"),
        "httpRequestStream" => Some("dream_http_request_stream"),
        "httpRequestStreamBytes" => Some("dream_http_request_stream_bytes"),
        "httpReadChunk" => Some("dream_http_read_chunk"),
        "httpCloseStream" => Some("dream_http_close_stream"),
        "tcpConnect" => Some("dream_tcp_connect"),
        "tcpSend" => Some("dream_tcp_send"),
        "tcpSendText" => Some("dream_tcp_send_text"),
        "tcpReceive" => Some("dream_tcp_receive"),
        "tcpClose" => Some("dream_tcp_close"),
        "wsConnect" => Some("dream_ws_connect"),
        "wsSendText" => Some("dream_ws_send_text"),
        "wsSendBinary" => Some("dream_ws_send_binary"),
        "wsReceive" => Some("dream_ws_receive"),
        "wsClose" => Some("dream_ws_close"),
        "gpuIsAvailable" => Some("dream_gpu_is_available"),
        "gpuReady" => Some("dream_gpu_ready"),
        "gpuLastError" => Some("dream_gpu_last_error"),
        "gpuTryInit" => Some("dream_gpu_try_init"),
        "gpuFrame" => Some("dream_gpu_frame"),
        "gpuTimestamp" => Some("dream_gpu_timestamp"),
        "gpuBufferAllocBytes" => Some("dream_gpu_buffer_alloc_bytes"),
        "gpuBufferAllocVertexBytes" => Some("dream_gpu_buffer_alloc_vertex_bytes"),
        "gpuBufferWriteBytes" => Some("dream_gpu_buffer_write_bytes"),
        "gpuBufferWriteBytesAt" => Some("dream_gpu_buffer_write_bytes_at"),
        "gpuBufferReadBytes" => Some("dream_gpu_buffer_read_bytes"),
        "gpuBufferReadBytesAt" => Some("dream_gpu_buffer_read_bytes_at"),
        "gpuBufferCopy" => Some("dream_gpu_buffer_copy"),
        "gpuBufferDestroy" => Some("dream_gpu_buffer_destroy"),
        "gpuDispatch" => Some("dream_gpu_dispatch"),
        "gpuDispatchIndirect" => Some("dream_gpu_dispatch_indirect"),
        "gpuShaderFromWgsl" => Some("dream_gpu_shader_from_wgsl"),
        "gpuDispatchShader" => Some("dream_gpu_dispatch_shader"),
        "gpuSamplerCreate" => Some("dream_gpu_sampler_create"),
        "gpuSamplerCreateEx" => Some("dream_gpu_sampler_create_ex"),
        "gpuTextureCreateRgba8" => Some("dream_gpu_texture_create_rgba8"),
        "gpuTextureCreateDepth" => Some("dream_gpu_texture_create_depth"),
        "gpuTextureCreateRgba16Float" => Some("dream_gpu_texture_create_rgba16_float"),
        "gpuTextureCreateCubeRgba8" => Some("dream_gpu_texture_create_cube_rgba8"),
        "gpuTextureWriteRgba" => Some("dream_gpu_texture_write_rgba"),
        "gpuTextureReadRgba" => Some("dream_gpu_texture_read_rgba"),
        "gpuTextureCopyFromBuffer" => Some("dream_gpu_texture_copy_from_buffer"),
        "gpuTextureCopyToBuffer" => Some("dream_gpu_texture_copy_to_buffer"),
        "gpuTextureCopy" => Some("dream_gpu_texture_copy"),
        "gpuTextureGenerateMipmaps" => Some("dream_gpu_texture_generate_mipmaps"),
        "gpuTextureDestroy" => Some("dream_gpu_texture_destroy"),
        "gpuSamplerDestroy" => Some("dream_gpu_sampler_destroy"),
        "gpuSurfaceFromCanvas" => Some("dream_gpu_surface_from_canvas"),
        "gpuSurfaceCreate" => Some("dream_gpu_surface_create"),
        "gpuSurfaceConfigure" => Some("dream_gpu_surface_configure"),
        "gpuSurfacePresent" => Some("dream_gpu_surface_present"),
        "gpuSurfacePointer" => Some("dream_gpu_surface_pointer"),
        "gpuSurfaceMods" => Some("dream_gpu_surface_mods"),
        "gpuSurfaceKeyDown" => Some("dream_gpu_surface_key_down"),
        "gpuSurfaceGamepads" => Some("dream_gpu_surface_gamepads"),
        "gpuSurfaceGamepadConnected" => Some("dream_gpu_surface_gamepad_connected"),
        "gpuSurfaceGamepadButtonDown" => Some("dream_gpu_surface_gamepad_button_down"),
        "gpuSurfaceGamepadAxis" => Some("dream_gpu_surface_gamepad_axis"),
        "gpuSurfaceFocused" => Some("dream_gpu_surface_focused"),
        "gpuSurfaceCloseRequested" => Some("dream_gpu_surface_close_requested"),
        "gpuSurfacePollEvents" => Some("dream_gpu_surface_poll_events"),
        "gpuSurfaceWidth" => Some("dream_gpu_surface_width"),
        "gpuSurfaceHeight" => Some("dream_gpu_surface_height"),
        "gpuSurfaceDestroy" => Some("dream_gpu_surface_destroy"),
        "gpuRenderBlit" => Some("dream_gpu_render_blit"),
        "gpuRenderPipelineCreate" => Some("dream_gpu_render_pipeline_create"),
        "gpuRenderPipelineCreateEx" => Some("dream_gpu_render_pipeline_create_ex"),
        "gpuRenderDraw" => Some("dream_gpu_render_draw"),
        "gpuRenderDrawEx" => Some("dream_gpu_render_draw_ex"),
        "gpuRenderDrawIndexed" => Some("dream_gpu_render_draw_indexed"),
        "gpuRenderDrawIndexedEx" => Some("dream_gpu_render_draw_indexed_ex"),
        "gpuRenderDrawTo" => Some("dream_gpu_render_draw_to"),
        "gpuRenderPipelineDestroy" => Some("dream_gpu_render_pipeline_destroy"),
        "gpuPassBegin" => Some("dream_gpu_pass_begin"),
        "gpuPassDispatch" => Some("dream_gpu_pass_dispatch"),
        "gpuPassDispatchIndirect" => Some("dream_gpu_pass_dispatch_indirect"),
        "gpuPassSubmit" => Some("dream_gpu_pass_submit"),
        "workerSpawn" => Some("dream_worker_spawn"),
        "workerPost" => Some("dream_worker_post"),
        "workerRecv" => Some("dream_worker_recv"),
        "workerTerminate" => Some("dream_worker_terminate"),
        "workerPoolSpawn" => Some("dream_worker_pool_spawn"),
        "workerPoolDispatch" => Some("dream_worker_pool_dispatch"),
        "webviewCreate" => Some("dream_webview_create"),
        "webviewLoadUrl" => Some("dream_webview_load_url"),
        "webviewLoadHtml" => Some("dream_webview_load_html"),
        "webviewLoadFile" => Some("dream_webview_load_file"),
        "webviewClose" => Some("dream_webview_close"),
        "webviewCloseRequested" => Some("dream_webview_close_requested"),
        "webviewTick" => Some("dream_webview_tick"),
        "webviewPoll" => Some("dream_webview_poll"),
        "webviewReply" => Some("dream_webview_reply"),
        "webviewReplyErr" => Some("dream_webview_reply_err"),
        "webviewReplyBytes" => Some("dream_webview_reply_bytes"),
        "webviewEmit" => Some("dream_webview_emit"),
        "webviewEmitBytes" => Some("dream_webview_emit_bytes"),
        "webviewEval" => Some("dream_webview_eval"),
        _ => None,
    }
}

pub(crate) fn is_c_runtime_sym(key: &str) -> bool {
    key.starts_with("dream_") || key.starts_with("debug_")
}

pub(crate) fn llvm_fn_name(sym: &str) -> String {
    let mut o = String::from("d_");
    for c in sym.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            o.push(c);
        } else {
            o.push('_');
        }
    }
    o
}

pub(crate) fn llvm_val_ty(interner: &TypeInterner, ty: TypeId) -> &'static str {
    match interner.kind(ty) {
        TyKind::Void => "void",
        TyKind::Prim(PrimTy::Double) => "double",
        TyKind::Prim(PrimTy::Long | PrimTy::ULong) => "i64",
        TyKind::Prim(PrimTy::Float) => "float",
        _ => "i32",
    }
}

pub(crate) fn zero(ty: &str) -> &'static str {
    match ty {
        "double" | "float" => "0x0000000000000000",
        "i64" => "0",
        _ => "0",
    }
}

pub(crate) const RUNTIME_DECLS: &str = r#"
declare i32 @dream_malloc(i32, i32)
declare void @dream_retain(i32)
declare void @dream_release(i32)
declare void @dream_retain_shared(i32)
declare void @dream_release_shared(i32)
declare void @dream_print_int(i32)
declare void @dream_print_uint(i32)
declare void @dream_print_long(i64)
declare void @dream_print_ulong(i64)
declare void @dream_print_float(float)
declare void @dream_print_double(double)
declare void @dream_print_char(i32)
declare void @dream_print_string(i32)
declare void @dream_print_newline()
declare void @dream_panic(i32)
declare i32 @dream_intern_utf8(i8*, i32)
declare i32 @dream_str_byte_size(i32)
declare i32 @dream_str_scalar_len(i32)
declare i32 @dream_string_eq(i32, i32)
declare i32 @dream_concat_strings(i32, i32)
declare i32 @dream_char_at(i32, i32)
declare i32 @dream_byte_at(i32, i32)
declare i32 @dream_array_len(i32)
declare i32 @dream_load_i32(i32)
declare void @dream_store_i32(i32, i32)
declare i8 @dream_load_u8(i32)
declare void @dream_store_u8(i32, i8)
declare i64 @dream_load_i64(i32)
declare void @dream_store_i64(i32, i64)
declare float @dream_load_f32(i32)
declare void @dream_store_f32(i32, float)
declare double @dream_load_f64(i32)
declare void @dream_store_f64(i32, double)
declare void @dream_memzero(i32, i32)
declare void @dream_memcpy(i32, i32, i32)
declare i32 @dream_realloc(i32, i32, i32)
declare i32 @dream_object_tag(i32)
declare i32 @dream_i32_to_string(i32)
declare i32 @dream_i64_to_string(i64)
declare i32 @dream_hash_bytes(i32)
declare void @dream_lock_acquire(i32)
declare void @dream_lock_release(i32)
declare i32 @dream_lock_try_acquire(i32)
declare i32 @dream_lock_try_acquire_for(i32, i32)
declare void @dream_sem_acquire(i32)
declare void @dream_sem_release(i32)
declare i32 @dream_sem_try_acquire(i32)
declare i32 @dream_sem_try_acquire_for(i32, i32)
declare void @dream_free(i32)
declare void @dream_unimplemented(i8*)
declare i32 @dream_box_i32(i32, i32)
declare i32 @dream_unbox_i32(i32)
declare i32 @dream_box_i64(i64, i32)
declare i64 @dream_unbox_i64(i32)
declare i32 @dream_box_f32(float)
declare float @dream_unbox_f32(i32)
declare i32 @dream_box_f64(double)
declare double @dream_unbox_f64(i32)
declare i32 @dream_f32_to_string(float)
declare i32 @dream_f64_to_string(double)
declare i32 @dream_bool_to_string(i32)
declare void @dream_print_bool(i32)
declare i32 @debug_get_live_objects()
declare i32 @debug_get_total_allocations()
declare i32 @debug_get_heap_ptr()
declare i32 @debug_get_free_list_head()
declare i32 @debug_get_ref_count(i32)
declare i32 @dream_string_alloc(i32)
declare i32 @dream_string_from_utf8(i32)
declare i32 @dream_string_from_utf8_prefix(i32, i32)
declare void @dream_string_copy_utf8(i32, i32, i32, i32, i32)
declare i32 @dream_string_clone(i32)
declare i32 @dream_string_compare(i32, i32)
declare i32 @dream_string_substring_raw(i32, i32, i32)
declare void @dream_string_set(i32, i32, i32)
declare i32 @dream_utf8_decode_at(i32, i32)
declare i32 @dream_utf8_width_at(i32, i32)
declare void @dream_simd_f32x4_add(i32, i32, i32, i32, i32, i32)
declare void @dream_simd_f32x4_sub(i32, i32, i32, i32, i32, i32)
declare void @dream_simd_f32x4_mul(i32, i32, i32, i32, i32, i32)
declare void @dream_simd_i32x4_add(i32, i32, i32, i32, i32, i32)
declare i64 @dream_nano_time()
declare i64 @dream_now_millis()
declare i32 @dream_date_local_offset_minutes(i64)
declare i32 @dream_date_zone_offset_minutes(i32, i64)
declare i32 @dream_date_local_zone_name()
declare double @dream_math_abs(double)
declare double @dream_math_floor(double)
declare double @dream_math_ceil(double)
declare double @dream_math_round(double)
declare double @dream_math_sqrt(double)
declare double @dream_math_pow(double, double)
declare double @dream_math_sin(double)
declare double @dream_math_cos(double)
declare double @dream_math_tan(double)
declare double @dream_math_asin(double)
declare double @dream_math_acos(double)
declare double @dream_math_atan(double)
declare double @dream_math_atan2(double, double)
declare i32 @dream_file_read(i32)
declare i64 @dream_file_write(i32, i32)
declare i64 @dream_file_append(i32, i32)
declare i32 @dream_file_read_bytes(i32)
declare i64 @dream_file_write_bytes(i32, i32)
declare i32 @dream_file_exists(i32)
declare i32 @dream_file_delete(i32)
declare i64 @dream_file_size(i32)
declare i32 @dream_file_is_dir(i32)
declare i32 @dream_dir_list(i32)
declare i32 @dream_dir_create(i32)
declare i32 @dream_dir_create_all(i32)
declare i32 @dream_process_platform()
declare i32 @dream_process_os_family()
declare i32 @dream_process_cwd()
declare i32 @dream_process_set_cwd(i32)
declare i32 @dream_process_args()
declare i32 @dream_process_env_get(i32)
declare void @dream_process_env_set(i32, i32)
declare i32 @dream_process_exe_path()
declare void @dream_console_exit(i32)
declare i32 @dream_console_read_line()
declare i32 @dream_console_read_key()
declare void @dream_delay_ms(i32)
declare i32 @dream_file_open(i32, i32)
declare i32 @dream_file_handle_read(i32, i32)
declare i64 @dream_file_handle_write(i32, i32)
declare i32 @dream_file_handle_seek(i32, i64)
declare void @dream_file_handle_close(i32)
declare i32 @dream_process_run(i32, i32, i32)
declare i32 @dream_process_spawn(i32, i32, i32)
declare i32 @dream_process_write_stdin(i32, i32)
declare i32 @dream_process_read_stream(i32, i32, i32)
declare i32 @dream_process_read_stream_line(i32, i32)
declare i32 @dream_process_wait(i32)
declare i32 @dream_process_kill(i32)
declare i32 @dream_crypto_sha256(i32)
declare i32 @dream_crypto_sha512(i32)
declare i32 @dream_crypto_hmac_sha256(i32, i32)
declare i32 @dream_crypto_secure_random_bytes(i32)
declare void @dream_crypto_secure_random_fill(i32)
declare i32 @dream_crypto_aes_gcm_encrypt(i32, i32, i32, i32)
declare i32 @dream_crypto_aes_gcm_decrypt(i32, i32, i32, i32)
declare i32 @dream_unicode_normalize(i32, i32)
declare i32 @dream_unicode_to_lower(i32)
declare i32 @dream_unicode_to_upper(i32)
declare i32 @dream_unicode_graphemes(i32)
declare i32 @dream_http_request(i32, i32, i32, i32, i32, i32)
declare i32 @dream_http_request_bytes(i32, i32, i32, i32, i32, i32)
declare i32 @dream_http_request_stream(i32, i32, i32, i32, i32, i32)
declare i32 @dream_http_request_stream_bytes(i32, i32, i32, i32, i32, i32)
declare i32 @dream_http_read_chunk(i32, i32)
declare i32 @dream_http_close_stream(i32)
declare i32 @dream_tcp_connect(i32, i32, i32)
declare i32 @dream_tcp_send(i32, i32)
declare i32 @dream_tcp_send_text(i32, i32)
declare i32 @dream_tcp_receive(i32, i32)
declare i32 @dream_tcp_close(i32)
declare i32 @dream_ws_connect(i32, i32)
declare i32 @dream_ws_send_text(i32, i32)
declare i32 @dream_ws_send_binary(i32, i32)
declare i32 @dream_ws_receive(i32)
declare i32 @dream_ws_close(i32, i32, i32)
declare i32 @dream_gpu_is_available()
declare i32 @dream_gpu_ready()
declare i32 @dream_gpu_last_error()
declare i32 @dream_gpu_try_init()
declare i32 @dream_gpu_frame()
declare i64 @dream_gpu_timestamp()
declare i32 @dream_gpu_buffer_alloc_bytes(i32)
declare i32 @dream_gpu_buffer_alloc_vertex_bytes(i32)
declare void @dream_gpu_buffer_write_bytes(i32, i32)
declare void @dream_gpu_buffer_write_bytes_at(i32, i32, i32)
declare i32 @dream_gpu_buffer_read_bytes(i32, i32)
declare i32 @dream_gpu_buffer_read_bytes_at(i32, i32, i32)
declare void @dream_gpu_buffer_copy(i32, i32, i32, i32, i32)
declare void @dream_gpu_buffer_destroy(i32)
declare i32 @dream_gpu_dispatch(i32, i32, i32, i32, i32, i32, i32, i32)
declare i32 @dream_gpu_dispatch_indirect(i32, i32, i32, i32, i32, i32)
declare i32 @dream_gpu_shader_from_wgsl(i32, i32)
declare i32 @dream_gpu_dispatch_shader(i32, i32, i32, i32, i32)
declare i32 @dream_gpu_sampler_create(i32)
declare i32 @dream_gpu_sampler_create_ex(i32, i32, i32)
declare i32 @dream_gpu_texture_create_rgba8(i32, i32)
declare i32 @dream_gpu_texture_create_depth(i32, i32)
declare i32 @dream_gpu_texture_create_rgba16_float(i32, i32)
declare i32 @dream_gpu_texture_create_cube_rgba8(i32)
declare i32 @dream_gpu_texture_write_rgba(i32, i32, i32, i32, i32, i32)
declare i32 @dream_gpu_texture_read_rgba(i32)
declare void @dream_gpu_texture_copy_from_buffer(i32, i32, i32, i32, i32, i32, i32)
declare void @dream_gpu_texture_copy_to_buffer(i32, i32, i32, i32, i32, i32, i32)
declare void @dream_gpu_texture_copy(i32, i32, i32, i32, i32, i32, i32, i32)
declare i32 @dream_gpu_texture_generate_mipmaps(i32)
declare void @dream_gpu_texture_destroy(i32)
declare void @dream_gpu_sampler_destroy(i32)
declare i32 @dream_gpu_surface_from_canvas(i32)
declare i32 @dream_gpu_surface_create(i32, i32, i32)
declare void @dream_gpu_surface_configure(i32, i32, i32)
declare i32 @dream_gpu_surface_present(i32)
declare i32 @dream_gpu_surface_pointer(i32)
declare i32 @dream_gpu_surface_mods(i32)
declare i32 @dream_gpu_surface_key_down(i32, i32)
declare i32 @dream_gpu_surface_gamepads(i32)
declare i32 @dream_gpu_surface_gamepad_connected(i32, i32)
declare i32 @dream_gpu_surface_gamepad_button_down(i32, i32, i32)
declare float @dream_gpu_surface_gamepad_axis(i32, i32, i32)
declare i32 @dream_gpu_surface_focused(i32)
declare i32 @dream_gpu_surface_close_requested(i32)
declare i32 @dream_gpu_surface_poll_events(i32)
declare i32 @dream_gpu_surface_width(i32)
declare i32 @dream_gpu_surface_height(i32)
declare void @dream_gpu_surface_destroy(i32)
declare i32 @dream_gpu_render_blit(i32, i32)
declare i32 @dream_gpu_render_pipeline_create(i32, i32)
declare i32 @dream_gpu_render_pipeline_create_ex(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32)
declare i32 @dream_gpu_render_draw(i32, i32, i32, i32, i32, float, float, float, float)
declare i32 @dream_gpu_render_draw_ex(i32, i32, i32, i32, i32, i32, float, float, float, float, i32, i32)
declare i32 @dream_gpu_render_draw_indexed(i32, i32, i32, i32, i32, i32, float, float, float, float)
declare i32 @dream_gpu_render_draw_indexed_ex(i32, i32, i32, i32, i32, i32, i32, float, float, float, float, i32, i32)
declare i32 @dream_gpu_render_draw_to(i32, i32, i32, i32, i32, i32, float, float, float, float, i32, i32)
declare void @dream_gpu_render_pipeline_destroy(i32)
declare i32 @dream_gpu_pass_begin()
declare void @dream_gpu_pass_dispatch(i32, i32, i32, i32, i32, i32, i32, i32, i32)
declare void @dream_gpu_pass_dispatch_indirect(i32, i32, i32, i32, i32, i32, i32)
declare i32 @dream_gpu_pass_submit(i32)
declare i32 @dream_worker_spawn(i32, i32)
declare void @dream_worker_post(i32, i32)
declare i32 @dream_worker_recv(i32)
declare void @dream_worker_terminate(i32)
declare i32 @dream_worker_pool_spawn()
declare i32 @dream_worker_pool_dispatch(i32, i32, i32, i32)
declare i32 @dream_task_join_if(i32)
declare void @dream_cancel(i32)
declare i32 @dream_task_run0(i32 ()*)
declare i32 @dream_task_run1(i32 (i32)*, i32)
declare i32 @dream_task_run2(i32 (i32, i32)*, i32, i32)
declare i32 @dream_task_run3(i32 (i32, i32, i32)*, i32, i32, i32)
declare i32 @dream_webview_create(i32, i32, i32)
declare i32 @dream_webview_load_url(i32, i32)
declare i32 @dream_webview_load_html(i32, i32)
declare i32 @dream_webview_load_file(i32, i32)
declare void @dream_webview_close(i32)
declare i32 @dream_webview_close_requested(i32)
declare i32 @dream_webview_tick(i32)
declare i32 @dream_webview_poll(i32)
declare void @dream_webview_reply(i32, i32, i32)
declare void @dream_webview_reply_err(i32, i32, i32)
declare void @dream_webview_reply_bytes(i32, i32, i32)
declare void @dream_webview_emit(i32, i32, i32)
declare void @dream_webview_emit_bytes(i32, i32, i32)
declare i32 @dream_webview_eval(i32, i32)
declare i64 @dream_c_invoke(i8*, i8*, i32, i32)
"#;

pub(crate) const DEBUG_DECLS: &str = r#"
declare void @dream_debug_enter(i32)
declare void @dream_debug_exit(i32)
declare void @dream_debug_line(i32, i32)
"#;

pub(crate) const DEBUG_DECLS_WASM: &str = r#"
declare void @dream_debug_enter(i32) #0
declare void @dream_debug_exit(i32) #1
declare void @dream_debug_line(i32, i32) #2
"#;

pub(crate) const DEBUG_ATTRS: &str = r#"
attributes #0 = { nounwind "wasm-import-module"="dream_debug" "wasm-import-name"="enter" }
attributes #1 = { nounwind "wasm-import-module"="dream_debug" "wasm-import-name"="exit" }
attributes #2 = { nounwind "wasm-import-module"="dream_debug" "wasm-import-name"="line" }
"#;


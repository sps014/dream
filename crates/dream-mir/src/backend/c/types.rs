use crate::backend::c::ctx::Cx;
use crate::BinOp;
use dream_types::{PrimTy, TyKind, TypeId};
use std::collections::HashSet;
use std::sync::OnceLock;

const NATIVE_RT_HEADER: &str = include_str!("../../runtime/c/native/include/dream_rt_native.h");

/// Names declared in `dream_rt_native.h` (including `static inline` helpers).
pub(super) fn native_header_declares(name: &str) -> bool {
    static NAMES: OnceLock<HashSet<String>> = OnceLock::new();
    NAMES.get_or_init(parse_native_header_fns).contains(name)
}

fn parse_native_header_fns() -> HashSet<String> {
    let mut names = HashSet::new();
    for raw in NATIVE_RT_HEADER.lines() {
        let line = raw.trim();
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with("//")
            || line.starts_with('*')
        {
            continue;
        }
        let Some(paren) = line.find('(') else {
            continue;
        };
        let before = line[..paren].trim();
        let ident = before
            .split_whitespace()
            .last()
            .unwrap_or("")
            .trim_start_matches('*');
        if !ident.is_empty() && ident.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            names.insert(ident.to_string());
        }
    }
    names
}

pub(super) fn c_ident(name: &str) -> String {
    let mut s = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_alphanumeric() || c == '_' {
            s.push(c);
        } else {
            s.push('_');
        }
        if i == 0 && s.as_bytes()[0].is_ascii_digit() {
            s.insert(0, '_');
        }
    }
    if s.is_empty() {
        s.push('_');
    }
    if s == "main" {
        return "main_dream".into();
    }
    s
}

pub(super) fn local_c_ty(interner: &dream_types::TypeInterner, ty: TypeId) -> &'static str {
    match interner.kind(ty) {
        TyKind::Prim(PrimTy::Int | PrimTy::UInt) => "int64_t",
        _ => c_ty(interner, ty),
    }
}

pub(super) fn c_ty(interner: &dream_types::TypeInterner, ty: TypeId) -> &'static str {
    match interner.kind(ty) {
        TyKind::Void => "void",
        TyKind::Prim(PrimTy::Double) => "double",
        TyKind::Prim(PrimTy::Float) => "float",
        TyKind::Prim(PrimTy::Long | PrimTy::ULong) => "int64_t",
        TyKind::Prim(PrimTy::Int | PrimTy::UInt | PrimTy::Bool | PrimTy::Char | PrimTy::Byte)
        | TyKind::Enum(_) => "int32_t",
        _ => "dream_ptr",
    }
}

pub(super) fn elem_size(cx: &Cx<'_>, ty: TypeId) -> u32 {
    native_scalar_size(cx, ty).0
}

pub(super) fn native_scalar_size(cx: &Cx<'_>, ty: TypeId) -> (u32, u32) {
    if cx.interner.is_value_type(ty) {
        if let Some(l) = cx.nstruct(ty) {
            return (l.size.max(1), 8);
        }
        if let Some(u) = cx.nunion(ty) {
            return (u.size.max(1), 8);
        }
    }
    match cx.interner.kind(ty) {
        TyKind::Prim(PrimTy::String) => (8, 8),
        TyKind::Prim(p) => p.size_align(),
        TyKind::Enum(_) => (4, 4),
        _ => (8, 8),
    }
}

pub(super) fn array_elem_ty(interner: &dream_types::TypeInterner, arr_ty: TypeId) -> TypeId {
    match interner.kind(arr_ty) {
        TyKind::Array(e) => *e,
        _ => arr_ty,
    }
}

pub(super) fn load_cast(cx: &Cx<'_>, ty: TypeId) -> &'static str {
    match cx.interner.kind(ty) {
        TyKind::Prim(PrimTy::Double) => "double",
        TyKind::Prim(PrimTy::Float) => "float",
        TyKind::Prim(PrimTy::Long | PrimTy::ULong) => "int64_t",
        TyKind::Prim(PrimTy::Byte | PrimTy::Bool | PrimTy::Char) => "uint8_t",
        TyKind::Prim(PrimTy::Int | PrimTy::UInt) | TyKind::Enum(_) => "int32_t",
        _ if cx.interner.is_value_type(ty) => "int32_t",
        _ => "dream_ptr",
    }
}

pub(super) fn bin(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Rem => "%",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::And => "&&",
        BinOp::Or => "||",
        BinOp::BitAnd => "&",
        BinOp::BitOr => "|",
        BinOp::BitXor => "^",
        BinOp::Shl => "<<",
        BinOp::Shr => ">>",
    }
}

pub(super) fn runtime_c_name(sym: &str) -> String {
    match sym {
        "malloc" => "dream_malloc".into(),
        "realloc" => "dream_realloc".into(),
        "free" | "force_free" => "dream_free".into(),
        "retain" | "retain_shared" => "dream_retain".into(),
        "release_generic" | "release_object" | "js_release" => "dream_release".into(),
        "release_funcbox" => "dream_release_funcbox".into(),
        "js_retain" => "dream_retain".into(),
        "concat_strings" => "dream_concat_strings".into(),
        "str_scalar_len" => "dream_str_len".into(),
        "str_byte_size" => "dream_str_byte_size".into(),
        "string_eq" => "dream_string_eq".into(),
        "string_alloc" => "dream_string_alloc".into(),
        "substring" => "dream_substring".into(),
        "array_new" => "dream_array_new".into(),
        "array_realloc" => "dream_array_realloc".into(),
        "to_bytes" => "dream_to_bytes".into(),
        "from_bytes" => "dream_from_bytes".into(),
        "char_at" => "dream_char_at".into(),
        "byte_at" => "dream_byte_at".into(),
        "hash_value" => "dream_hash_value".into(),
        "dream_panic" => "dream_panic".into(),
        "print_object" => "dream_print_object".into(),
        "object_to_string" => "dream_object_to_string".into(),
        "object_hash_code" => "dream_object_hash_code".into(),
        "object_tag" => "dream_object_tag".into(),
        "int_to_string" => "dream_int_to_string".into(),
        "uint_to_string" => "dream_uint_to_string".into(),
        "long_to_string" => "dream_long_to_string".into(),
        "ulong_to_string" => "dream_ulong_to_string".into(),
        "byte_to_string" => "dream_byte_to_string".into(),
        "bool_to_string" => "dream_bool_to_string".into(),
        "char_to_string" => "dream_char_to_string".into(),
        "float_to_string" => "dream_float_to_string".into(),
        "double_to_string" => "dream_double_to_string".into(),
        "funcbox_new" => "dream_funcbox_new".into(),
        "funcbox_funcidx" => "dream_funcbox_funcidx".into(),
        "funcbox_env" => "dream_funcbox_env".into(),
        "box_int" => "dream_box_int".into(),
        "box_float" => "dream_box_float".into(),
        "box_double" => "dream_box_double".into(),
        "box_bool" => "dream_box_bool".into(),
        "box_char" => "dream_box_char".into(),
        "box_long" => "dream_box_long".into(),
        "box_uint" => "dream_box_uint".into(),
        "box_ulong" => "dream_box_ulong".into(),
        "box_byte" => "dream_box_byte".into(),
        "unbox_int" => "dream_unbox_int".into(),
        "unbox_float" => "dream_unbox_float".into(),
        "unbox_double" => "dream_unbox_double".into(),
        "unbox_bool" => "dream_unbox_bool".into(),
        "unbox_char" => "dream_unbox_char".into(),
        "unbox_long" => "dream_unbox_long".into(),
        "unbox_uint" => "dream_unbox_uint".into(),
        "unbox_ulong" => "dream_unbox_ulong".into(),
        "unbox_byte" => "dream_unbox_byte".into(),
        "__lock_acquire" => "dream_lock_acquire".into(),
        "__lock_release" => "dream_lock_release".into(),
        "dream_complete" => "dream_async_complete".into(),
        "sleep" => "dream_sleep".into(),
        "dream_cancel" => "dream_cancel".into(),
        "__promise_all" | "promise_all" | "dream_all" => "dream_all".into(),
        "__promise_any" | "__promise_race" | "promise_any" | "promise_race" | "dream_any" => {
            "dream_any".into()
        }
        "utf8_width_at" => "utf8_width_at".into(),
        "utf8_decode_at" => "utf8_decode_at".into(),
        "__lock_try_acquire" => "dream_lock_try_acquire".into(),
        "__lock_try_acquire_for" => "dream_lock_try_acquire_for".into(),
        "shared_lock_acquire" => "dream_lock_acquire".into(),
        "shared_lock_release" => "dream_lock_release".into(),
        "shared_lock_try_acquire" => "dream_lock_try_acquire".into(),
        "shared_lock_try_acquire_for" => "dream_lock_try_acquire_for".into(),
        "shared_semaphore_acquire" => "dream_semaphore_acquire".into(),
        "shared_semaphore_release" => "dream_semaphore_release".into(),
        "shared_semaphore_try_acquire" => "dream_semaphore_try_acquire".into(),
        "shared_semaphore_try_acquire_for" => "dream_semaphore_try_acquire_for".into(),
        "debug_get_ref_count" => "debug_get_ref_count".into(),
        "debug_get_heap_ptr" => "debug_get_heap_ptr".into(),
        "debug_get_free_list_head" => "debug_get_free_list_head".into(),
        "abs" => "dream_host_abs".into(),
        other => c_ident(other),
    }
}

pub(super) fn import_host_name(imp: &dream_hir::HImport) -> String {
    if !imp.field.is_empty() {
        runtime_c_name(&imp.field)
    } else {
        runtime_c_name(&imp.name)
    }
}

pub(super) fn import_is_async_future(_mir: &crate::Mir, imp: &dream_hir::HImport) -> bool {
    imp.is_async
}

pub(super) fn import_call_name(mir: &crate::Mir, imp: &dream_hir::HImport) -> String {
    let host = import_host_name(imp);
    if import_is_async_future(mir, imp) {
        format!("__async_{host}")
    } else {
        host
    }
}

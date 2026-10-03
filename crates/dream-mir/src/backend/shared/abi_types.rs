//! Value classes, sizes and symbol names the generated code and the C runtime agree on: a Dream
//! value crosses the runtime ABI as one of [`AbiTy`], generated symbols are spelled by
//! [`c_ident`], and runtime entry points are named by [`runtime_c_name`].

use super::cx::Cx;
use dream_types::{PrimTy, TyKind, TypeId, TypeInterner};
use indexmap::IndexSet as HashSet;
use std::sync::OnceLock;

const NATIVE_RT_HEADER: &str = include_str!("../../runtime/c/native/include/dream_rt_native.h");

/// The ABI class of a Dream value (the runtime's `int32_t`/`int64_t`/`float`/`double`/
/// `dream_ptr`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum AbiTy {
    Void,
    I32,
    I64,
    Word,
    F32,
    F64,
    Ptr,
}

impl AbiTy {
    /// The token used in shared function-pointer signature names (`dream_fn_i32_ptr__v`).
    pub(crate) fn token(self) -> &'static str {
        match self {
            AbiTy::Void => "v",
            AbiTy::I32 => "i32",
            AbiTy::I64 => "i64",
            AbiTy::Word => "word",
            AbiTy::F32 => "f32",
            AbiTy::F64 => "f64",
            AbiTy::Ptr => "ptr",
        }
    }

    pub(crate) fn is_wide(self) -> bool {
        matches!(self, AbiTy::I64 | AbiTy::Word | AbiTy::Ptr | AbiTy::F64)
    }
}

pub(crate) fn abi_ty(interner: &TypeInterner, ty: TypeId) -> AbiTy {
    match interner.kind(ty) {
        TyKind::Void => AbiTy::Void,
        TyKind::Prim(PrimTy::Double) => AbiTy::F64,
        TyKind::Prim(PrimTy::Float) => AbiTy::F32,
        TyKind::Prim(PrimTy::Long | PrimTy::ULong) => AbiTy::I64,
        TyKind::Prim(PrimTy::ISize | PrimTy::USize) => AbiTy::Word,
        TyKind::Prim(PrimTy::Int | PrimTy::UInt | PrimTy::Bool | PrimTy::Char | PrimTy::Byte)
        | TyKind::Enum(_) => AbiTy::I32,
        _ => AbiTy::Ptr,
    }
}

/// How a value of a type is read from memory: the in-memory width can be narrower than its
/// [`AbiTy`] (`bool`/`byte`/`char` fields are one byte, zero-extended on load).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MemTy {
    U8,
    I32,
    I64,
    Word,
    UWord,
    F32,
    F64,
    Ptr,
}

pub(crate) fn mem_ty(cx: &Cx<'_>, ty: TypeId) -> MemTy {
    match cx.interner.kind(ty) {
        TyKind::Prim(PrimTy::Double) => MemTy::F64,
        TyKind::Prim(PrimTy::Float) => MemTy::F32,
        TyKind::Prim(PrimTy::Long | PrimTy::ULong) => MemTy::I64,
        TyKind::Prim(PrimTy::ISize) => MemTy::Word,
        TyKind::Prim(PrimTy::USize) => MemTy::UWord,
        TyKind::Prim(PrimTy::Byte | PrimTy::Bool | PrimTy::Char) => MemTy::U8,
        TyKind::Prim(PrimTy::Int | PrimTy::UInt) | TyKind::Enum(_) => MemTy::I32,
        _ if cx.interner.is_value_type(ty) => MemTy::I32,
        _ => MemTy::Ptr,
    }
}

/// `(params, ret)` ABI classes of a function type, plus its shared signature name.
pub(crate) fn fn_sig(interner: &TypeInterner, ty: TypeId) -> (String, AbiTy, Vec<AbiTy>) {
    match interner.kind(ty) {
        TyKind::Func(params, ret) => {
            // Native returns value structs as `dream_ptr` (heap copy), not WASM sret.
            let ps: Vec<AbiTy> = params.iter().map(|p| abi_ty(interner, *p)).collect();
            let r = abi_ty(interner, *ret);
            let params = if ps.is_empty() {
                "void".to_string()
            } else {
                ps.iter().map(|t| t.token()).collect::<Vec<_>>().join("_")
            };
            (format!("dream_fn_{params}__{}", r.token()), r, ps)
        }
        other => crate::internal_error!("fn_sig on non-function type {other:?}"),
    }
}

/// Names declared in `dream_rt_native.h` (including `static inline` helpers).
pub(crate) fn native_header_declares(name: &str) -> bool {
    static NAMES: OnceLock<HashSet<String>> = OnceLock::new();
    NAMES.get_or_init(parse_native_header_fns).contains(name)
}

/// Every function name the native runtime header declares, sorted.
pub(crate) fn native_header_fn_names() -> Vec<String> {
    let mut v: Vec<String> = parse_native_header_fns().into_iter().collect();
    v.sort();
    v
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

pub(crate) fn c_ident(name: &str) -> String {
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

/// `int`/`uint` locals that are assigned a pointer-sized value. Closure envs and task ids are
/// stored in `int` locals; on native those bits do not fit in 32 bits.
pub(crate) fn wide_int_locals(cx: &Cx<'_>, func: &crate::MirFunction) -> Vec<bool> {
    let n = func.locals.len();
    let mut wide = vec![false; n];
    if cx.target.is_wasm32() {
        return wide;
    }
    let mut changed = true;
    while changed {
        changed = false;
        for block in &func.blocks {
            for stmt in &block.stmts {
                let crate::Statement::Assign(crate::Place::Local(d), rv) = stmt else {
                    continue;
                };
                let di = d.0 as usize;
                if wide[di] || !is_narrow_int(cx, func.local_ty(*d)) {
                    continue;
                }
                if rvalue_is_wide(cx, func, &wide, rv) {
                    wide[di] = true;
                    changed = true;
                }
            }
        }
    }
    wide
}

fn is_narrow_int(cx: &Cx<'_>, ty: TypeId) -> bool {
    matches!(
        cx.interner.kind(ty),
        TyKind::Prim(PrimTy::Int | PrimTy::UInt)
    )
}

fn ty_is_wide(cx: &Cx<'_>, ty: TypeId) -> bool {
    abi_ty(cx.interner, ty).is_wide()
}

fn operand_is_wide(
    cx: &Cx<'_>,
    func: &crate::MirFunction,
    wide: &[bool],
    op: &crate::Operand,
) -> bool {
    match op {
        crate::Operand::Const(crate::Const::Long(_) | crate::Const::Float(_)) => true,
        crate::Operand::Copy(crate::Place::Local(l)) => {
            let ty = func.local_ty(*l);
            ty_is_wide(cx, ty)
                || (is_narrow_int(cx, ty) && wide.get(l.0 as usize).copied().unwrap_or(false))
        }
        crate::Operand::Copy(crate::Place::Global(g)) => {
            cx.global_ty(*g).is_some_and(|ty| ty_is_wide(cx, ty))
        }
        crate::Operand::Copy(crate::Place::Field { base, field }) => cx
            .nstruct(func.local_ty(*base))
            .and_then(|layout| layout.fields.get(*field))
            .is_some_and(|f| ty_is_wide(cx, f.ty)),
        crate::Operand::Copy(crate::Place::Index { base, .. }) => {
            ty_is_wide(cx, array_elem_ty(cx.interner, func.local_ty(*base)))
        }
        crate::Operand::Copy(crate::Place::Deref { elem_ty, .. }) => ty_is_wide(cx, *elem_ty),
        crate::Operand::Const(_) => false,
    }
}

fn rvalue_is_wide(
    cx: &Cx<'_>,
    func: &crate::MirFunction,
    wide: &[bool],
    rv: &crate::Rvalue,
) -> bool {
    let op = |o: &crate::Operand| operand_is_wide(cx, func, wide, o);
    match rv {
        crate::Rvalue::Use(o)
        | crate::Rvalue::Unary(_, o)
        | crate::Rvalue::CheckedNeg(o)
        | crate::Rvalue::ArrayLen(o)
        | crate::Rvalue::StrLen(o)
        | crate::Rvalue::StrByteSize(o)
        | crate::Rvalue::Discriminant { base: o, .. }
        | crate::Rvalue::HashCode(o)
        | crate::Rvalue::ToString(o)
        | crate::Rvalue::TypeName(o)
        | crate::Rvalue::IsType(o, _) => op(o),
        crate::Rvalue::UnionField {
            ty, variant, field, ..
        } => cx
            .nunion(*ty)
            .and_then(|u| u.variants.get(*variant))
            .and_then(|v| v.fields.get(*field))
            .is_some_and(|f| ty_is_wide(cx, f.ty)),
        crate::Rvalue::Binary(_, a, b)
        | crate::Rvalue::CheckedBinary(_, a, b)
        | crate::Rvalue::CharAt(a, b, _)
        | crate::Rvalue::ByteAt(a, b, _) => op(a) || op(b),
        // The address is a widened pointer. The loaded unit is an `int`.
        crate::Rvalue::LoadU8(_, _) | crate::Rvalue::LoadU16(_, _) => false,
        crate::Rvalue::StrBytes(_) => true,
        crate::Rvalue::Select {
            cond,
            then_val,
            else_val,
        } => op(cond) || op(then_val) || op(else_val),
        crate::Rvalue::Cast(o, _, to) => ty_is_wide(cx, *to) || op(o),
        crate::Rvalue::Call { callee, args } => {
            // `funcbox_env` is typed as `int` but returns a host pointer.
            cx.intrinsic_key(callee.def)
                .is_some_and(|key| key == "funcbox_env" || key == "funcbox_new")
                || ty_is_wide(cx, callee.ret)
                || args.iter().any(op)
        }
        crate::Rvalue::IndirectCall { args, .. } => args.iter().any(op),
        crate::Rvalue::InterfaceCall {
            receiver,
            ret,
            args,
            ..
        } => ty_is_wide(cx, *ret) || op(receiver) || args.iter().any(op),
        crate::Rvalue::New { ty, .. }
        | crate::Rvalue::UnionNew { ty, .. }
        | crate::Rvalue::ArrayNew { elem_ty: ty, .. }
        | crate::Rvalue::ArrayLit { elem_ty: ty, .. } => ty_is_wide(cx, *ty),
        _ => false,
    }
}

pub(crate) fn elem_size(cx: &Cx<'_>, ty: TypeId) -> u32 {
    native_scalar_size(cx, ty).0
}

pub(crate) fn native_scalar_size(cx: &Cx<'_>, ty: TypeId) -> (u32, u32) {
    let (size, align) = cx.mir.layouts.size_align(cx.interner, ty);
    (size.max(1), align)
}

pub(crate) fn array_elem_ty(interner: &TypeInterner, arr_ty: TypeId) -> TypeId {
    match interner.kind(arr_ty) {
        TyKind::Array(e) => *e,
        _ => arr_ty,
    }
}

pub(crate) fn runtime_c_name(sym: &str) -> String {
    match sym {
        "malloc" => "dream_malloc".into(),
        "realloc" => "dream_realloc".into(),
        "free" | "force_free" => "dream_free".into(),
        "retain" | "retain_shared" => "dream_retain".into(),
        "release_generic" | "release_object" | "js_release" => "dream_release".into(),
        "release_funcbox" => "dream_release_funcbox".into(),
        "js_retain" => "dream_retain".into(),
        "concat_strings" => "dream_concat_strings".into(),
        "concat_str_int_str" => "dream_concat_str_int_str".into(),
        "str_scalar_len" => "dream_str_len".into(),
        "str_byte_size" => "dream_str_byte_size".into(),
        "string_eq" => "dream_string_eq".into(),
        "string_alloc" => "dream_string_alloc".into(),
        "substring" | "string_substring_raw" => "dream_substring".into(),
        "string_builder_push" => "dream_sb_push".into(),
        "string_builder_push_int" => "dream_sb_push_int".into(),
        "string_builder_push_long" => "dream_sb_push_long".into(),
        "array_new" => "dream_array_new".into(),
        "array_realloc" => "dream_array_realloc".into(),
        "to_bytes" => "dream_to_bytes".into(),
        "from_bytes" => "dream_from_bytes".into(),
        "char_at" => "dream_char_at".into(),
        "byte_at" => "dream_byte_at".into(),
        "dream_panic" => "dream_panic".into(),
        "print_object" => "dream_print_object".into(),
        "object_to_string" => "dream_object_to_string".into(),
        "object_hash_code" => "dream_object_hash_code".into(),
        "object_tag" => "dream_object_tag".into(),
        "int_to_string" => "dream_int_to_string_fast".into(),
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
        "dream_start" => "dream_start".into(),
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
        "log" => "dream_host_log".into(),
        "log10" => "dream_host_log10".into(),
        "exp" => "dream_host_exp".into(),
        "hypot" => "dream_host_hypot".into(),
        other => c_ident(other),
    }
}

pub(crate) fn import_host_name(imp: &dream_hir::HImport) -> String {
    if is_c_import(imp) {
        return if imp.field.is_empty() {
            imp.name.clone()
        } else {
            imp.field.clone()
        };
    }
    if imp.module == "dream_ffi" {
        let field = if imp.field.is_empty() {
            c_ident(&imp.name)
        } else {
            c_ident(&imp.field)
        };
        return format!("dream_ffi_{field}");
    }
    if !imp.field.is_empty() {
        runtime_c_name(&imp.field)
    } else {
        runtime_c_name(&imp.name)
    }
}

pub(crate) fn import_call_name(imp: &dream_hir::HImport) -> String {
    let base = if imp.module.starts_with("c/") {
        // Keyed by the Dream extern rather than the C symbol: several externs may bind one
        // symbol with different Dream-side types (`free` for two handle classes).
        format!("dream_c_{}", c_ident(&imp.name))
    } else {
        import_host_name(imp)
    };
    if imp.is_async {
        format!("__async_{base}")
    } else {
        base
    }
}

pub(crate) fn is_c_import(imp: &dream_hir::HImport) -> bool {
    imp.module.starts_with("c/")
}

/// A deferred-extern poll symbol for `imp`, or `None` when the import is not an async host bridge
/// (C FFI imports keep their synchronous wrappers).
pub(crate) fn lazy_import_poll(imp: &dream_hir::HImport) -> Option<String> {
    if is_c_import(imp) || !imp.is_async {
        return None;
    }
    Some(format!("{}_poll", import_call_name(imp)))
}

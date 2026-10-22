//! Value classes, sizes and symbol names the generated code and the C runtime agree on: a Dream
//! value crosses the runtime ABI as one of [`AbiTy`], generated symbols are spelled by
//! [`c_ident`], and runtime entry points are named by [`runtime_c_name`].

use super::cx::Cx;
use dream_types::{PrimTy, TyKind, TypeId, TypeInterner};
use indexmap::IndexSet as HashSet;
use std::sync::OnceLock;

const NATIVE_RT_HEADER: &str = include_str!("../../runtime/c/core/include/dream_core.h");

#[cfg(test)]
#[path = "ref_local_tests.rs"]
mod ref_local_tests;

/// The ABI class of a Dream value (the runtime's `int32_t`/`int64_t`/`float`/`double`/
/// `dream_ptr`, plus LLVM `half` for `float16`, which no runtime entry point takes directly).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum AbiTy {
    Void,
    I32,
    I64,
    Word,
    F16,
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
            AbiTy::F16 => "f16",
            AbiTy::F32 => "f32",
            AbiTy::F64 => "f64",
            AbiTy::Ptr => "ptr",
        }
    }
}

pub(crate) fn abi_ty(interner: &TypeInterner, ty: TypeId) -> AbiTy {
    match interner.kind(ty) {
        TyKind::Void => AbiTy::Void,
        TyKind::Prim(PrimTy::Double) => AbiTy::F64,
        TyKind::Prim(PrimTy::Float) => AbiTy::F32,
        TyKind::Prim(PrimTy::Float16) => AbiTy::F16,
        TyKind::Prim(PrimTy::Long | PrimTy::ULong) => AbiTy::I64,
        TyKind::Prim(PrimTy::ISize | PrimTy::USize) => AbiTy::Word,
        TyKind::Prim(PrimTy::Int | PrimTy::UInt | PrimTy::Bool | PrimTy::Char | PrimTy::Byte)
        | TyKind::Enum(_)
        | TyKind::Js => AbiTy::I32,
        _ => AbiTy::Ptr,
    }
}

/// How a value of a type is read from memory: the in-memory width can be narrower than its
/// [`AbiTy`] (`bool`/`byte` fields are one byte, zero-extended on load).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MemTy {
    U8,
    I32,
    I64,
    Word,
    UWord,
    F16,
    F32,
    F64,
    Ptr,
}

pub(crate) fn mem_ty(cx: &Cx<'_>, ty: TypeId) -> MemTy {
    match cx.interner.kind(ty) {
        TyKind::Prim(PrimTy::Double) => MemTy::F64,
        TyKind::Prim(PrimTy::Float) => MemTy::F32,
        TyKind::Prim(PrimTy::Float16) => MemTy::F16,
        TyKind::Prim(PrimTy::Long | PrimTy::ULong) => MemTy::I64,
        TyKind::Prim(PrimTy::ISize) => MemTy::Word,
        TyKind::Prim(PrimTy::USize) => MemTy::UWord,
        TyKind::Prim(PrimTy::Byte | PrimTy::Bool) => MemTy::U8,
        TyKind::Prim(PrimTy::Char | PrimTy::Int | PrimTy::UInt) | TyKind::Enum(_) | TyKind::Js => {
            MemTy::I32
        }
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

/// Names declared in `dream_core.h` (including `static inline` helpers).
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
    for raw in NATIVE_RT_HEADER
        .lines()
        .chain(include_str!("../../runtime/c/core/include/dream_ownership.h").lines())
        .chain(include_str!("../../runtime/c/include/dream_gpu_bundle.h").lines())
    {
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

/// Synthetic `int` locals carrying addresses rather than language integers.
/// Result classification must not inherit a call's argument representation: lengths,
/// selectors and worker IDs remain integers even when their inputs are references.
pub(crate) fn ref_int_locals(cx: &Cx<'_>, func: &crate::MirFunction) -> Vec<bool> {
    let mut wide: Vec<bool> = func.locals.iter().map(|local| local.is_ref).collect();
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
    abi_ty(cx.interner, ty) == AbiTy::Ptr
}

fn operand_is_wide(
    cx: &Cx<'_>,
    func: &crate::MirFunction,
    wide: &[bool],
    op: &crate::Operand,
) -> bool {
    match op {
        crate::Operand::Copy(crate::Place::Local(l)) => {
            let ty = func.local_ty(*l);
            ty_is_wide(cx, ty)
                || (is_narrow_int(cx, ty) && wide.get(l.0 as usize).copied().unwrap_or(false))
        }
        crate::Operand::Copy(crate::Place::Global(g)) => {
            g.0 == 0 || cx.global_ty(*g).is_some_and(|ty| ty_is_wide(cx, ty))
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
        crate::Rvalue::Use(o) => op(o),
        crate::Rvalue::Move { src, cast } => {
            cast.is_none_or(|(_, to)| is_narrow_int(cx, to) || ty_is_wide(cx, to))
                && operand_is_wide(
                    cx,
                    func,
                    wide,
                    &crate::Operand::Copy(crate::Place::Local(*src)),
                )
        }
        crate::Rvalue::ToString(_) | crate::Rvalue::TypeName(_) => true,
        crate::Rvalue::UnionField {
            ty, variant, field, ..
        } => cx
            .nunion(*ty)
            .and_then(|u| u.variants.get(*variant))
            .and_then(|v| v.fields.get(*field))
            .is_some_and(|f| ty_is_wide(cx, f.ty)),
        crate::Rvalue::Binary(crate::BinOp::Add | crate::BinOp::Sub, a, b) => op(a) || op(b),
        // The address is a widened pointer. The loaded unit is an `int`.
        crate::Rvalue::LoadU8(..) | crate::Rvalue::LoadU16(..) => false,
        crate::Rvalue::StrBytes(_) => true,
        crate::Rvalue::Select {
            then_val, else_val, ..
        } => op(then_val) || op(else_val),
        crate::Rvalue::Cast(o, _, to) => ty_is_wide(cx, *to) || (is_narrow_int(cx, *to) && op(o)),
        crate::Rvalue::Zero(ty) => ty_is_wide(cx, *ty),
        crate::Rvalue::Call { callee, .. } => {
            // `funcbox_env` is typed as `int` but returns a host pointer.
            cx.intrinsic_key(callee.def).is_some_and(|key| {
                matches!(
                    dream_abi::intrinsics::IntrinsicOp::from_key(key),
                    Some(
                        dream_abi::intrinsics::IntrinsicOp::FuncboxEnv
                            | dream_abi::intrinsics::IntrinsicOp::FuncboxNew
                    )
                )
            }) || ty_is_wide(cx, callee.ret)
        }
        crate::Rvalue::IndirectCall { sig, .. } => match cx.interner.kind(*sig) {
            TyKind::Func(_, ret) => ty_is_wide(cx, *ret),
            _ => false,
        },
        crate::Rvalue::InterfaceCall { ret, .. } => ty_is_wide(cx, *ret),
        crate::Rvalue::New { .. }
        | crate::Rvalue::UnionNew { .. }
        | crate::Rvalue::ArrayNew { .. }
        | crate::Rvalue::ArrayLit { .. }
        | crate::Rvalue::ToBytes { .. }
        | crate::Rvalue::FromBytes { .. } => true,
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
        "string_alloc" => "dream_string_alloc".into(),
        dream_abi::intrinsics::ATTR_STRING_SUBSTRING => "dream_substring".into(),
        dream_abi::intrinsics::ATTR_STRING_BUILDER_BUFFER => "dream_sb_buffer".into(),
        "string_builder_push" => "dream_sb_push".into(),
        "string_builder_push_int" => "dream_sb_push_int".into(),
        "string_builder_push_long" => "dream_sb_push_long".into(),
        dream_abi::intrinsics::ATTR_STRING_BUILDER_PUSH_DOUBLE => "dream_sb_push_double".into(),
        dream_abi::intrinsics::ATTR_FUNCBOX_NEW => "dream_funcbox_new".into(),
        dream_abi::intrinsics::ATTR_FUNCBOX_FUNCIDX => "dream_funcbox_funcidx".into(),
        dream_abi::intrinsics::ATTR_FUNCBOX_ENV => "dream_funcbox_env".into(),
        dream_abi::intrinsics::SLEEP => "dream_sleep".into(),
        dream_abi::intrinsics::ATTR_PROMISE_ALL => "dream_all".into(),
        dream_abi::intrinsics::ATTR_PROMISE_RACE => "dream_race".into(),
        "shared_lock_acquire" => "dream_lock_acquire".into(),
        "shared_lock_release" => "dream_lock_release".into(),
        "shared_lock_try_acquire" => "dream_lock_try_acquire".into(),
        "shared_lock_try_acquire_for" => "dream_lock_try_acquire_for".into(),
        "shared_semaphore_acquire" => "dream_semaphore_acquire".into(),
        "shared_semaphore_release" => "dream_semaphore_release".into(),
        "shared_semaphore_try_acquire" => "dream_semaphore_try_acquire".into(),
        "shared_semaphore_try_acquire_for" => "dream_semaphore_try_acquire_for".into(),
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

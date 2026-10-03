//! Which function renders, hashes or tags a value of each type. The IR writers only decide how the
//! call is spelled.

use super::abi_types::c_ident;
use super::cx::Cx;
use dream_types::{PrimTy, TyKind, TypeId};

/// The `to_string` conversion for `ty`; empty for `string`, which is already its own text.
pub(crate) fn to_string_fn(cx: &Cx<'_>, ty: TypeId) -> String {
    match cx.interner.kind(ty) {
        TyKind::Prim(PrimTy::Int) => "dream_int_to_string_fast".into(),
        TyKind::Prim(PrimTy::UInt) => "dream_uint_to_string".into(),
        TyKind::Prim(PrimTy::Long) => "dream_long_to_string".into(),
        TyKind::Prim(PrimTy::ULong) => "dream_ulong_to_string".into(),
        TyKind::Prim(PrimTy::ISize) => if cx.mir.layouts.target.ptr_size == 8 {
            "dream_long_to_string"
        } else {
            "dream_int_to_string_fast"
        }
        .into(),
        TyKind::Prim(PrimTy::USize) => if cx.mir.layouts.target.ptr_size == 8 {
            "dream_ulong_to_string"
        } else {
            "dream_uint_to_string"
        }
        .into(),
        TyKind::Prim(PrimTy::Byte) => "dream_byte_to_string".into(),
        TyKind::Prim(PrimTy::Bool) => "dream_bool_to_string".into(),
        TyKind::Prim(PrimTy::Char) => "dream_char_to_string".into(),
        TyKind::Prim(PrimTy::Float) => "dream_float_to_string".into(),
        TyKind::Prim(PrimTy::Double) => "dream_double_to_string".into(),
        TyKind::Prim(PrimTy::String) => String::new(),
        TyKind::Enum(_) => "dream_int_to_string".into(),
        TyKind::Array(e) => c_ident(&format!("array_to_string_t{}", e.0)),
        _ => {
            if let Some(l) = cx.nstruct(ty) {
                c_ident(&format!("{}_to_string", l.name))
            } else if let Some(u) = cx.nunion(ty) {
                c_ident(&format!("{}_to_string", u.name))
            } else {
                "dream_object_to_string".into()
            }
        }
    }
}

pub(crate) enum HashFn {
    /// The value's own `int32` bits.
    Identity,
    Call(String),
}

pub(crate) fn hash_fn(cx: &Cx<'_>, ty: TypeId) -> HashFn {
    match cx.interner.kind(ty) {
        TyKind::Prim(PrimTy::String) => HashFn::Call("dream_string_hash".into()),
        TyKind::Prim(PrimTy::Float) => HashFn::Call("dream_bitcast_f32".into()),
        TyKind::Prim(PrimTy::Double) => HashFn::Call("dream_hash_double".into()),
        TyKind::Prim(PrimTy::Long | PrimTy::ULong) => HashFn::Call("dream_hash_long".into()),
        TyKind::Prim(PrimTy::ISize | PrimTy::USize) if cx.mir.layouts.target.ptr_size == 8 => {
            HashFn::Call("dream_hash_long".into())
        }
        TyKind::Prim(_) | TyKind::Enum(_) => HashFn::Identity,
        _ => HashFn::Call(if let Some(l) = cx.nstruct(ty) {
            c_ident(&format!("{}_hash_code", l.name))
        } else if let Some(u) = cx.nunion(ty) {
            c_ident(&format!("{}_hash_code", u.name))
        } else {
            "dream_object_hash_code".into()
        }),
    }
}

/// The heap tag a boxed value of `ty` carries.
pub(crate) fn runtime_tag(cx: &Cx<'_>, ty: TypeId) -> i32 {
    match cx.interner.kind(ty) {
        TyKind::Prim(PrimTy::Int) => crate::abi::TAG_INT,
        TyKind::Prim(PrimTy::Float) => crate::abi::TAG_FLOAT,
        TyKind::Prim(PrimTy::Double) => crate::abi::TAG_DOUBLE,
        TyKind::Prim(PrimTy::Bool) => crate::abi::TAG_BOOL,
        TyKind::Prim(PrimTy::String) => crate::abi::TAG_STRING,
        TyKind::Prim(PrimTy::Char) => crate::abi::TAG_CHAR,
        TyKind::Prim(PrimTy::Long) => crate::abi::TAG_LONG,
        TyKind::Prim(PrimTy::UInt) => crate::abi::TAG_UINT,
        TyKind::Prim(PrimTy::ULong) => crate::abi::TAG_ULONG,
        TyKind::Prim(PrimTy::ISize) => crate::abi::TAG_ISIZE,
        TyKind::Prim(PrimTy::USize) => crate::abi::TAG_USIZE,
        TyKind::Prim(PrimTy::Byte) => crate::abi::TAG_BYTE,
        TyKind::Array(_) => crate::abi::TAG_ARRAY,
        _ => cx.type_tag(ty),
    }
}

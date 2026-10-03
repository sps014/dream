//! Dream types as LLVM first-class types. The mapping is the runtime's C ABI classes exactly
//! (`int32_t` / `int64_t` / `float` / `double` / `dream_ptr`), with `dream_ptr` as `i64` on
//! native and `i32` on wasm32.

use super::fx::Fx;
use super::ir::{FnTy, Ty};
use super::lcx::FnSig;
use crate::backend::shared::abi_types::{abi_ty, fn_sig, AbiTy};
use crate::{Local, MirFunction};
use dream_types::{TypeId, TypeInterner};

/// `h` is the target's `dream_ptr` type (`Lcx::h`).
pub(super) fn abi_ll(t: AbiTy, h: &Ty) -> Ty {
    match t {
        AbiTy::Void => Ty::Void,
        AbiTy::I32 => Ty::I32,
        AbiTy::I64 => Ty::I64,
        AbiTy::Ptr => h.clone(),
        AbiTy::Word => h.clone(),
        AbiTy::F32 => Ty::F32,
        AbiTy::F64 => Ty::F64,
    }
}

pub(super) fn ll_ty(interner: &TypeInterner, ty: TypeId, h: &Ty) -> Ty {
    abi_ll(abi_ty(interner, ty), h)
}

/// `dream_ptr` is `uintptr_t`; every other C local type is signed.
pub(super) fn is_unsigned(interner: &TypeInterner, ty: TypeId) -> bool {
    abi_ty(interner, ty) == AbiTy::Ptr
        || matches!(
            interner.kind(ty),
            dream_types::TyKind::Prim(dream_types::PrimTy::USize)
        )
}

/// The signature every generated Dream function has.
pub(super) fn fn_ll_sig(interner: &TypeInterner, f: &MirFunction, h: &Ty) -> FnSig {
    let ret = if f.is_async {
        h.clone()
    } else {
        ll_ty(interner, f.ret, h)
    };
    let params = f
        .params
        .iter()
        .map(|p| ll_ty(interner, f.local_ty(*p), h))
        .collect();
    FnSig::plain(FnTy::new(ret, params))
}

/// A `fun(...)` type's function-pointer signature.
pub(super) fn fn_ptr_sig(interner: &TypeInterner, sig: TypeId, h: &Ty) -> FnSig {
    let (_, ret, params) = fn_sig(interner, sig);
    FnSig::plain(FnTy::new(
        abi_ll(ret, h),
        params.into_iter().map(|p| abi_ll(p, h)).collect(),
    ))
}

impl<'l, 'a> Fx<'l, 'a> {
    pub fn local_ll(&self, l: Local) -> Ty {
        if self.wide.get(l.0 as usize).copied().unwrap_or(false) {
            Ty::I64
        } else {
            ll_ty(self.interner, self.f.local_ty(l), &self.h())
        }
    }

    pub fn is_value(&self, ty: TypeId) -> bool {
        self.interner.is_value_type(ty)
    }

    pub fn is_rc(&self, ty: TypeId) -> bool {
        self.interner.is_rc_tracked(ty)
    }
}

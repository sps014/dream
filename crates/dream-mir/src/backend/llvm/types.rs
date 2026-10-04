//! Dream types as LLVM first-class types. The mapping is the runtime's C ABI classes exactly
//! (`int32_t` / `int64_t` / `float` / `double` / `dream_ptr`), with pointer references on
//! native and linear-memory offsets on wasm32. Target-sized integers have a separate class.

use super::fx::Fx;
use super::ir::{FnTy, Ty};
use super::lcx::FnSig;
use crate::backend::shared::abi_types::{abi_ty, fn_sig, AbiTy};
use crate::{Local, MirFunction};
use dream_types::{TypeId, TypeInterner};

/// `h` is the target's `dream_ptr` type (`Lcx::h`).
pub(super) fn abi_ll(t: AbiTy, h: &Ty, word: &Ty) -> Ty {
    match t {
        AbiTy::Void => Ty::Void,
        AbiTy::I32 => Ty::I32,
        AbiTy::I64 => Ty::I64,
        AbiTy::Ptr => h.clone(),
        AbiTy::Word => word.clone(),
        AbiTy::F32 => Ty::F32,
        AbiTy::F64 => Ty::F64,
    }
}

pub(super) fn ll_ty(interner: &TypeInterner, ty: TypeId, h: &Ty, word: &Ty) -> Ty {
    abi_ll(abi_ty(interner, ty), h, word)
}

/// Offset references and unsigned target-sized integers zero-extend at integer boundaries.
pub(super) fn is_unsigned(interner: &TypeInterner, ty: TypeId) -> bool {
    abi_ty(interner, ty) == AbiTy::Ptr
        || matches!(
            interner.kind(ty),
            dream_types::TyKind::Prim(dream_types::PrimTy::USize)
        )
}

/// The signature every generated Dream function has.
pub(super) fn fn_ll_sig(interner: &TypeInterner, f: &MirFunction, h: &Ty, word: &Ty) -> FnSig {
    let ret = if f.is_async {
        h.clone()
    } else {
        ll_ty(interner, f.ret, h, word)
    };
    let params = f
        .params
        .iter()
        .map(|p| {
            if f.locals[p.0 as usize].is_ref {
                h.clone()
            } else {
                ll_ty(interner, f.local_ty(*p), h, word)
            }
        })
        .collect();
    FnSig::plain(FnTy::new(ret, params))
}

/// A `fun(...)` type's function-pointer signature.
pub(super) fn fn_ptr_sig(interner: &TypeInterner, sig: TypeId, h: &Ty, word: &Ty) -> FnSig {
    let (_, ret, params) = fn_sig(interner, sig);
    FnSig::plain(FnTy::new(
        abi_ll(ret, h, word),
        params.into_iter().map(|p| abi_ll(p, h, word)).collect(),
    ))
}

impl<'l, 'a> Fx<'l, 'a> {
    pub fn local_ll(&self, l: Local) -> Ty {
        if self.f.locals[l.0 as usize].is_ref
            || self.wide.get(l.0 as usize).copied().unwrap_or(false)
        {
            self.h()
        } else {
            ll_ty(self.interner, self.f.local_ty(l), &self.h(), &self.word())
        }
    }

    pub fn is_value(&self, ty: TypeId) -> bool {
        self.interner.is_value_type(ty)
    }

    pub fn is_rc(&self, ty: TypeId) -> bool {
        self.interner.is_rc_tracked(ty)
    }
}

pub(super) fn export_functions(l: &super::lcx::Lcx<'_>) -> Vec<dream_abi::exports::ExportFunction> {
    fn c_type(ty: Ty, source: TypeId, interner: &TypeInterner) -> &'static str {
        let unsigned = matches!(
            interner.kind(source),
            dream_types::TyKind::Prim(
                dream_types::PrimTy::UInt
                    | dream_types::PrimTy::ULong
                    | dream_types::PrimTy::USize
                    | dream_types::PrimTy::Bool
                    | dream_types::PrimTy::Byte
                    | dream_types::PrimTy::Char
            )
        );
        if matches!(ty, Ty::I32 | Ty::I64) {
            if matches!(
                interner.kind(source),
                dream_types::TyKind::Prim(dream_types::PrimTy::ISize | dream_types::PrimTy::USize)
            ) {
                return if unsigned { "uintptr_t" } else { "intptr_t" };
            }
            if unsigned {
                return if ty == Ty::I32 {
                    "uint32_t"
                } else {
                    "uint64_t"
                };
            }
        }
        match ty {
            Ty::Void => "void",
            Ty::I32 => "int32_t",
            Ty::I64 => "int64_t",
            Ty::F32 => "float",
            Ty::F64 => "double",
            Ty::Ptr => "void *",
            _ => crate::internal_error!("unsupported export ABI type"),
        }
    }
    let mut out = Vec::new();
    for (def, export) in &l.mir.exports {
        let f = l
            .mir
            .functions
            .iter()
            .find(|f| f.def == *def)
            .unwrap_or_else(|| crate::internal_error!("exported function was pruned"));
        let sig = fn_ll_sig(l.interner, f, &l.h(), &l.word());
        let classify = |abi: Ty, ty: TypeId| {
            let kind = if abi == Ty::Void {
                dream_abi::exports::ExportKind::Void
            } else if abi == Ty::Ptr {
                dream_abi::exports::ExportKind::Opaque
            } else {
                match l.interner.kind(ty) {
                    dream_types::TyKind::Prim(p) => {
                        use dream_abi::exports::ExportKind as K;
                        use dream_types::PrimTy as P;
                        match p {
                            P::Int => K::Int,
                            P::UInt => K::UInt,
                            P::Long => K::Long,
                            P::ULong => K::ULong,
                            P::Bool => K::Bool,
                            P::Byte => K::Byte,
                            P::Char => K::Char,
                            P::ISize => K::ISize,
                            P::USize => K::USize,
                            P::Float => K::Float,
                            P::Double => K::Double,
                            P::String => K::Opaque,
                        }
                    }
                    _ => dream_abi::exports::ExportKind::Int,
                }
            };
            dream_abi::exports::ExportType {
                c_type: c_type(abi, ty, l.interner).into(),
                kind,
            }
        };
        out.push(dream_abi::exports::ExportFunction {
            name: export.clone(),
            ret: classify(sig.fty.ret, f.ret),
            params: sig
                .fty
                .params
                .into_iter()
                .enumerate()
                .map(|(i, abi)| {
                    let local = &f.locals[f.params[i].0 as usize];
                    dream_abi::exports::ExportParam {
                        ty: classify(abi, local.ty),
                        take: local.is_take,
                        is_ref: local.is_ref,
                    }
                })
                .collect(),
        });
    }
    out
}

pub(super) fn export_header(exports: &[dream_abi::exports::ExportFunction]) -> String {
    let mut out = String::from("#pragma once\n#include <stdint.h>\n");
    out.push_str(include_str!("../../runtime/c/include/dream_platform.h"));
    out.push_str(
        &include_str!("../../runtime/c/include/dream_embed.h")
            .replace("#include \"dream_platform.h\"", ""),
    );
    out.push_str("\n#ifdef __cplusplus\nextern \"C\" {\n#endif\n");
    out.push_str("/* Take parameters consume one reference; borrow/ref parameters do not.\n * Returned references own one count. Attach each calling thread; serialize the first call. */\n");
    for f in exports {
        for (i, p) in f.params.iter().enumerate() {
            if p.ty.kind == dream_abi::exports::ExportKind::Opaque {
                let mode = if p.take { "take" } else { "borrow/ref" };
                out.push_str(&format!("/* arg{i}: {mode} */\n"));
            }
        }
        let params = if f.params.is_empty() {
            "void".to_string()
        } else {
            f.params
                .iter()
                .enumerate()
                .map(|(i, p)| format!("{} arg{i}", p.ty.c_type))
                .collect::<Vec<_>>()
                .join(", ")
        };
        out.push_str(&format!("{} {}({params});\n", f.ret.c_type, f.name));
    }
    out.push_str("#ifdef __cplusplus\n}\n#endif\n");
    out
}

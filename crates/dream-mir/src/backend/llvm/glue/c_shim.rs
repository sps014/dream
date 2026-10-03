//! Describes the module's `@c` crossings as a [`CShim`], which the driver compiles with clang and
//! links beside the module. Mirrors the declarations `c_marshal` and `c_reverse` emit.

use super::super::lcx::Lcx;
use super::c_marshal::{destructor_getter, shape, shim_name};
use super::c_reverse::{fun_parts, reverse_trampolines};
use crate::backend::shared::abi_types::{import_host_name, is_c_import};
use dream_abi::c_abi::shim::{CField, CShim, CStruct, CTy, Destructor, Forward, Reverse};
use dream_hir::CShape;
use dream_types::{CScalar, TyKind, TypeId};

pub(in crate::backend::llvm) fn build(l: &Lcx<'_>) -> CShim {
    let mut shim = CShim::default();
    for imp in l.mir.imports.iter().filter(|i| is_c_import(i)) {
        let mut params = Vec::new();
        for (i, ty) in imp.params.iter().enumerate() {
            match shape(imp, i) {
                CShape::Callback { .. } => params.extend([CTy::Ptr, CTy::Ptr]),
                s => params.push(c_ty(l, &mut shim, s, *ty)),
            }
        }
        let ret_ty = imp.ret.unwrap_or_else(|| l.interner.int());
        let ret = c_ty(l, &mut shim, &imp.c_ret, ret_ty);
        shim.forwards.push(Forward {
            shim: shim_name(imp),
            symbol: import_host_name(imp),
            stdcall: imp.c_stdcall,
            params,
            ret,
        });
        if let CShape::OwnedPtr { free } = &imp.c_ret {
            shim.destructors.push(Destructor {
                getter: destructor_getter(imp),
                symbol: free.clone(),
            });
        }
    }
    for (rev, (shapes, ret)) in reverse_trampolines(l) {
        let (param_tys, ret_ty) = fun_parts(l, rev.fun_ty());
        let mut params: Vec<CTy> = shapes
            .iter()
            .zip(&param_tys)
            .map(|(s, t)| c_ty(l, &mut shim, s, *t))
            .collect();
        if let Some(at) = rev.user_data_at(params.len()) {
            params.insert(at, CTy::Ptr);
        }
        let ret = c_ty(l, &mut shim, &ret, ret_ty);
        shim.reverses.push(Reverse {
            adapter: rev.symbol(l),
            body: rev.body(l),
            params,
            ret,
        });
    }
    shim
}

fn c_ty(l: &Lcx<'_>, shim: &mut CShim, s: &CShape, ty: TypeId) -> CTy {
    match s {
        CShape::Void => CTy::Void,
        CShape::Scalar(sc) => CTy::Scalar(*sc),
        CShape::Struct => CTy::Struct(c_struct(l, shim, ty)),
        _ => CTy::Ptr,
    }
}

/// The C mirror of the @unmanaged value struct `ty` (the analyzer admits only scalar and nested
/// value-struct fields).
fn c_struct(l: &Lcx<'_>, shim: &mut CShim, ty: TypeId) -> usize {
    let layout =
        l.cx.nstruct(ty)
            .unwrap_or_else(|| crate::internal_error!("by-value C struct without a layout"));
    let packed = layout.packed;
    let fields: Vec<(u32, TypeId)> = layout.fields.iter().map(|f| (f.offset, f.ty)).collect();
    let fields = fields
        .into_iter()
        .map(|(offset, fty)| {
            let field = match l.interner.kind(fty) {
                TyKind::Prim(p) => CScalar::of_prim(*p).map(CField::Scalar),
                TyKind::Struct(..) if l.interner.is_value_type(fty) => {
                    Some(CField::Struct(c_struct(l, shim, fty)))
                }
                _ => None,
            };
            let field = field.unwrap_or_else(|| {
                crate::internal_error!("by-value C struct field of non-C type {fty:?}")
            });
            (offset, field)
        })
        .collect();
    shim.intern_struct(CStruct { packed, fields })
}

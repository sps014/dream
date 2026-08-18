//! `@c("lib", "symbol")` trampolines: Dream types → C ABI, then the real symbol.

use super::ast::{CTy, Expr, Param, Stmt};
use super::builder::{FuncBuilder, ModuleBuilder};
use super::ctx::Cx;
use super::types::{c_ty, import_call_name, import_host_name};
use dream_hir::HImport;
use dream_types::{PrimTy, TyKind};

pub(super) fn is_c_import(imp: &HImport) -> bool {
    imp.module.starts_with("c/")
}

pub(super) fn emit_c_import(m: &mut ModuleBuilder, cx: &Cx<'_>, imp: &HImport) {
    let real = import_host_name(imp);
    let wrap = import_call_name(cx.mir, imp);
    let dream_ret = imp.ret.map(|t| c_ty(cx.interner, t)).unwrap_or(CTy::Void);
    let dream_params: Vec<Param> = imp
        .params
        .iter()
        .enumerate()
        .map(|(i, t)| Param {
            ty: c_ty(cx.interner, *t),
            name: format!("a{i}"),
        })
        .collect();
    let abi_params: Vec<Param> = imp
        .params
        .iter()
        .enumerate()
        .map(|(i, t)| Param {
            ty: c_abi_ty(cx, *t, is_ref(imp, i), imp.c_wide_strings),
            name: format!("p{i}"),
        })
        .collect();
    m.proto(dream_ret.clone(), real.clone(), abi_params);

    let mut b = FuncBuilder::new(dream_ret.clone(), wrap);
    b.params = dream_params;
    let mut real_args = Vec::new();
    let mut frees = Vec::new();
    for (i, ty) in imp.params.iter().enumerate() {
        let arg = Expr::id(format!("a{i}"));
        if is_ref(imp, i) {
            real_args.push(as_void_ptr(arg));
            continue;
        }
        match cx.interner.kind(*ty) {
            TyKind::Prim(PrimTy::String) => {
                let conv = if imp.c_wide_strings {
                    "dream_string_to_utf16z"
                } else {
                    "dream_string_to_utf8"
                };
                let tmp = format!("__s{i}");
                b.stmt(Stmt::decl(
                    CTy::VoidPtr,
                    tmp.clone(),
                    Some(Expr::cast(CTy::VoidPtr, Expr::call(conv, vec![arg]))),
                ));
                real_args.push(Expr::id(&tmp));
                frees.push(tmp);
            }
            TyKind::Func(..) => {
                real_args.push(Expr::call(
                    "dream_ft_get",
                    vec![Expr::call("dream_funcbox_funcidx", vec![arg])],
                ));
            }
            _ => real_args.push(arg),
        }
    }
    let call = Expr::call(real, real_args);
    if dream_ret == CTy::Void {
        b.expr_stmt(call);
        for f in &frees {
            b.call("free", vec![Expr::id(f)]);
        }
        b.ret(None);
    } else if matches!(
        imp.ret.map(|t| cx.interner.kind(t)),
        Some(TyKind::Prim(PrimTy::String))
    ) {
        b.stmt(Stmt::decl(
            CTy::CharPtr,
            "__r",
            Some(Expr::cast(CTy::CharPtr, call)),
        ));
        b.stmt(Stmt::decl(
            CTy::Ptr,
            "__ds",
            Some(Expr::call("dream_utf8_to_string", vec![Expr::id("__r")])),
        ));
        for f in &frees {
            b.call("free", vec![Expr::id(f)]);
        }
        b.ret(Some(Expr::id("__ds")));
    } else {
        b.stmt(Stmt::decl(dream_ret, "__r", Some(call)));
        for f in &frees {
            b.call("free", vec![Expr::id(f)]);
        }
        b.ret(Some(Expr::id("__r")));
    }
    m.push_func(b);
}

fn is_ref(imp: &HImport, i: usize) -> bool {
    imp.param_by_ref.get(i).copied().unwrap_or(false)
}

fn as_void_ptr(arg: Expr) -> Expr {
    Expr::cast(CTy::VoidPtr, Expr::cast(CTy::Named("uintptr_t"), arg))
}

fn c_abi_ty(cx: &Cx<'_>, ty: dream_types::TypeId, by_ref: bool, wide: bool) -> CTy {
    if by_ref {
        return CTy::VoidPtr;
    }
    match cx.interner.kind(ty) {
        TyKind::Prim(PrimTy::String) => {
            if wide {
                CTy::ptr_to(CTy::U16)
            } else {
                CTy::CharPtr
            }
        }
        TyKind::Func(..) => CTy::VoidPtr,
        _ => c_ty(cx.interner, ty),
    }
}

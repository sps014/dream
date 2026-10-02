//! MIR → textual LLVM IR for native and wasm32 targets. See `docs/internals/06-llvm-backend.md`.

mod body;
mod calls;
mod debug;
mod debug_views;
mod fx;
mod glue;
mod int_ops;
pub mod ir;
mod js;
mod lcx;
mod places;
mod publication;
pub mod runtime_sigs;
mod rvalue;
mod statements;
mod terminator;
mod types;
mod value_refs;

pub use runtime_sigs::RuntimeSigs;

use crate::Mir;
use dream_types::TypeInterner;
use ir::{FnTy, Ty};
use lcx::{FnSig, Lcx};
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};

#[derive(Debug, PartialEq, Eq)]
struct MissingRuntimeSymbol(String);

#[derive(Debug, PartialEq, Eq)]
pub enum EmitError {
    MissingRuntimeSymbol(String),
}

impl std::fmt::Display for EmitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingRuntimeSymbol(name) => {
                write!(f, "required runtime symbol `{name}` is missing")
            }
        }
    }
}

/// Function names the native runtime header declares; the driver references each from an anchor
/// unit so the runtime signature table covers header-declared host functions too.
pub fn native_header_function_names() -> Vec<String> {
    crate::backend::shared::abi_types::native_header_fn_names()
}

/// Lowers optimized MIR to one `.ll` module typed against the runtime's signature table.
/// `leak_checks` makes native `main` always print the exit-time heap report.
pub fn emit_llvm_module(
    mir: &Mir,
    interner: &TypeInterner,
    sigs: &RuntimeSigs,
    leak_checks: bool,
    target: crate::backend::Target,
) -> Result<String, EmitError> {
    let result = catch_unwind(AssertUnwindSafe(|| {
        emit_llvm_module_unchecked(mir, interner, sigs, leak_checks, target)
    }));
    match result {
        Ok(ir) => Ok(ir),
        Err(payload) => match payload.downcast::<MissingRuntimeSymbol>() {
            Ok(missing) => Err(EmitError::MissingRuntimeSymbol(missing.0)),
            Err(payload) => resume_unwind(payload),
        },
    }
}

fn emit_llvm_module_unchecked(
    mir: &Mir,
    interner: &TypeInterner,
    sigs: &RuntimeSigs,
    leak_checks: bool,
    target: crate::backend::Target,
) -> String {
    let mut l = Lcx::new(mir, interner, sigs, leak_checks, target);
    for f in &mir.functions {
        let name = l.user_fn(f);
        let sig = types::fn_ll_sig(interner, f, &l.h());
        if body::returns_via_buffer(&l, f, &name) {
            let mut direct = sig.clone();
            direct.fty.ret = Ty::Void;
            direct.fty.params.push(Ty::Ptr);
            direct.ret_attrs.clear();
            l.own(&format!("{name}__boxed"), sig);
            l.own(&name, direct);
            l.sret.insert(name);
            continue;
        }
        l.own(&name, sig);
        if f.is_async {
            let poll = body::poll_name(&l, f);
            let drop = body::drop_name(&l, f);
            let h = l.h();
            l.own(&poll, FnSig::plain(FnTy::new(Ty::I32, vec![h.clone()])));
            l.own(&drop, FnSig::plain(FnTy::new(Ty::Void, vec![h])));
        }
    }
    let reach = crate::backend::shared::reach::compute(&l.cx);
    let protocol = glue::protocol::plan(&l, &reach);
    glue::release::register_all(&mut l);
    glue::protocol::register_all(&mut l, &protocol);
    glue::tables::register_all(&mut l);
    glue::imports::register_all(&mut l);
    glue::js_marshal::register_all(&mut l);
    glue::entry::register_all(&mut l);

    glue::tables::emit_strings(&mut l);
    glue::tables::emit_globals(&mut l);
    let mut async_i = 0usize;
    for f in &mir.functions {
        if !f.is_async {
            body::build_sync(&mut l, f);
            body::build_boxed_wrapper(&mut l, f);
            continue;
        }
        let poll_idx = mir.functions.len() + 1 + async_i;
        let pre_lowered = &mir.polls[async_i];
        async_i += 1;
        if f.hir_fn.is_none() {
            body::build_sync(&mut l, f);
            body::build_empty_poll_drop(&mut l, f);
            continue;
        }
        let (offs, frame_size) = body::async_offsets(&l, pre_lowered);
        body::build_async_stub(&mut l, f, pre_lowered, &offs, frame_size, poll_idx as i32);
        body::build_poll(&mut l, f, pre_lowered, &offs);
        body::build_future_drop(&mut l, f, pre_lowered, &offs);
    }
    glue::release::emit_all(&mut l);
    glue::protocol::emit_all(&mut l, &protocol);
    glue::imports::emit_all(&mut l);
    glue::js_marshal::emit_all(&mut l);
    glue::tables::emit_all(&mut l);
    glue::entry::emit_all(&mut l);
    l.m.finish()
}

//! Module-level functions with no MIR body: ARC glue, protocol routers, dispatch tables, the
//! runtime hooks and the process entry.

pub(super) mod c_marshal;
pub(super) mod c_reverse;
pub(super) mod c_shim;
pub(super) mod entry;
pub(super) mod exports;
pub(super) mod imports;
pub(super) mod js_marshal;
pub(super) mod protocol;
pub(super) mod release;
pub(super) mod tables;

use super::fx::{Fx, V};
use super::ir::{FnTy, Ty};
use super::lcx::{FnSig, Lcx};
use crate::{BlockId, MirFunction};
use dream_types::{DefId, TypeId};

/// The (empty) MIR body a glue function's lowering state points at.
static GLUE_FN: MirFunction = MirFunction {
    def: DefId::root(0),
    instance: Vec::new(),
    name: String::new(),
    symbol: String::new(),
    params: Vec::new(),
    ret: TypeId(0),
    locals: Vec::new(),
    blocks: Vec::new(),
    entry: BlockId(0),
    is_async: false,
    hir_fn: None,
    file: None,
    inline: dream_hir::InlineHint::Default,
};

pub(super) fn register(l: &mut Lcx<'_>, name: &str, ret: Ty, params: Vec<Ty>) {
    l.own(name, FnSig::plain(FnTy::new(ret, params)));
}

/// A glue function positioned in a fresh body block (the entry block stays a plain prologue so
/// loops may branch back to the body).
pub(super) fn glue<'l, 'a>(l: &'l mut Lcx<'a>, name: &str) -> Fx<'l, 'a> {
    let w = l.writer(name);
    let mut fx = Fx::new(l, &GLUE_FN, w);
    let body = fx.w.new_block("body");
    fx.w.br(body);
    fx.w.switch_to(body);
    fx
}

impl<'l, 'a> Fx<'l, 'a> {
    pub(super) fn arg(&self, i: usize) -> V {
        let v = self.w.param(i);
        let unsigned = v.ty == Ty::I64;
        V { v, unsigned }
    }

    /// `if (!p) return;`
    pub(super) fn ret_if_null(&mut self, p: &V) {
        let z = self
            .w
            .icmp("eq", &p.v, &super::ir::Value::zero(p.ty().clone()));
        self.if_then(&z, |fx| fx.ret_default());
    }
}

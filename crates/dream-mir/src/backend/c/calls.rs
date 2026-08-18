use super::ast::{CTy, Expr};
use super::emit::Emitter;
use super::types::{c_ident, runtime_c_name};
use crate::{Callee, Operand};

impl<'a> Emitter<'a> {
    pub(super) fn call_expr(&mut self, callee: &Callee, args: &[Operand]) -> Expr {
        let raw = self.cx.callee_c(callee.def, &callee.args);
        let name = runtime_c_name(&raw);
        let mut args_e: Vec<Expr> = args.iter().map(|a| self.operand(a)).collect();
        if name == "dream_all" {
            let es = match self.cx.interner.kind(callee.ret) {
                dream_types::TyKind::Array(e) => super::types::elem_size(self.cx, *e),
                _ => callee
                    .args
                    .first()
                    .map(|t| super::types::elem_size(self.cx, *t))
                    .unwrap_or(4),
            };
            args_e.push(Expr::i(es as i64));
        }
        Expr::call(name, args_e)
    }

    pub(super) fn indirect_expr(&mut self, target: &Operand, args: &[Operand]) -> Expr {
        let mut args_e: Vec<Expr> = args.iter().map(|a| self.operand(a)).collect();
        while args_e.len() < 8 {
            args_e.push(Expr::i(0));
        }
        let t = self.operand(target);
        let idx = Expr::ternary(
            Expr::lt(
                Expr::cast(CTy::Named("uintptr_t"), t.clone()),
                Expr::UInt(65536),
            ),
            Expr::cast(CTy::I32, Expr::cast(CTy::Named("uintptr_t"), t.clone())),
            Expr::call(
                "dream_funcbox_funcidx",
                vec![Expr::cast(CTy::Ptr, t.clone())],
            ),
        );
        let fn_ptr = Expr::cast(
            CTy::Ident("dream_fn".into()),
            Expr::index(Expr::id("dream_ft"), idx),
        );
        Expr::IndirectCall {
            callee: Box::new(fn_ptr),
            args: args_e,
        }
    }

    pub(super) fn iface_expr(
        &mut self,
        receiver: &Operand,
        iface_id: usize,
        method_slot: usize,
        args: &[Operand],
    ) -> Expr {
        let mut all = vec![self.operand(receiver)];
        all.extend(args.iter().map(|a| self.operand(a)));
        while all.len() < 8 {
            all.push(Expr::i(0));
        }
        Expr::call(
            c_ident(&format!("__iface_dispatch_{iface_id}_{method_slot}")),
            all,
        )
    }

    pub(super) fn js_call_expr(
        &mut self,
        target: &Operand,
        via: &Option<Operand>,
        method: &Option<Operand>,
        args: &[(Operand, dream_types::TypeId)],
    ) -> Expr {
        let t = self.operand(target);
        let v = via
            .as_ref()
            .map(|o| self.operand(o))
            .unwrap_or_else(|| Expr::i(0));
        let m = method
            .as_ref()
            .map(|o| self.operand(o))
            .unwrap_or_else(|| Expr::i(0));
        Expr::call("dream_js_call", vec![t, v, m, Expr::i(args.len() as i64)])
    }
}

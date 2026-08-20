use super::ast::{CTy, Expr};
use super::emit::Emitter;
use super::types::{c_ident, runtime_c_name};
use crate::{Callee, Const, Operand};

impl<'a> Emitter<'a> {
    pub(super) fn call_expr(&mut self, callee: &Callee, args: &[Operand]) -> Expr {
        let raw = self.cx.callee_c(callee.def, &callee.args);
        let name = runtime_c_name(&raw);
        if name == "dream_sb_push" {
            if let Some(e) = self.sb_push_expr(args) {
                return e;
            }
        }
        let mut args_e: Vec<Expr> = Vec::with_capacity(args.len());
        for (i, a) in args.iter().enumerate() {
            self.retain_rc_global_sink(callee.take_params.get(i).copied().unwrap_or(false), a);
            args_e.push(self.operand(a));
        }
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

    pub(super) fn retain_rc_global_sink(&mut self, take: bool, arg: &Operand) {
        if !take {
            return;
        }
        let Operand::Copy(crate::Place::Global(g)) = arg else {
            return;
        };
        let Some(ty) = self
            .cx
            .mir
            .globals
            .iter()
            .find(|global| global.id == *g)
            .map(|global| global.ty)
        else {
            return;
        };
        if !self.cx.interner.is_rc_tracked(ty) {
            return;
        }
        let a = self.operand(arg);
        self.b.call("dream_retain", vec![a]);
    }

    fn sb_push_expr(&mut self, args: &[Operand]) -> Option<Expr> {
        if args.len() < 2 {
            return None;
        }
        let text = args.get(1)?;
        let s = match text {
            Operand::Const(Const::Str(s)) => s.as_str(),
            _ => return None,
        };
        let n = s.encode_utf16().count() as i64;
        if n <= 0 {
            return None;
        }
        let sym = self.cx.str_sym(s);
        Some(Expr::call(
            "dream_sb_push_units",
            vec![
                self.operand(&args[0]),
                Expr::cast(
                    CTy::VoidPtr,
                    Expr::ptr_add(
                        Expr::id(sym),
                        Expr::i(crate::abi::STRING_UNITS_OFFSET as i64),
                    ),
                ),
                Expr::i(n),
            ],
        ))
    }

    pub(super) fn indirect_expr(
        &mut self,
        target: &Operand,
        args: &[Operand],
        sig: dream_types::TypeId,
    ) -> Expr {
        let args_e: Vec<Expr> = args
            .iter()
            .map(|a| {
                self.retain_rc_global_sink(true, a);
                self.operand(a)
            })
            .collect();
        let (td, _, params) = super::types::fn_ptr_abi(self.cx.interner, sig);
        if args_e.len() != params.len() {
            crate::internal_error!(
                "indirect call arity {} != signature arity {}",
                args_e.len(),
                params.len()
            );
        }
        let t = self.operand(target);
        let idx = Expr::cast(CTy::I32, t);
        let fn_ptr = Expr::cast(CTy::Ident(td), Expr::index(Expr::id("dream_ft"), idx));
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
        for a in args {
            self.retain_rc_global_sink(true, a);
            all.push(self.operand(a));
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

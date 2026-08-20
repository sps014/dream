use super::ast::{CTy, Expr, Stmt};
use super::emit::Emitter;
use super::types::{c_ident, runtime_c_name};
use crate::{Callee, Const, Operand};
use dream_abi::js_abi;
use dream_types::TypeId;

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
        callee: &Callee,
        target: &Operand,
        via: &Option<Operand>,
        method: &Option<Operand>,
        args: &[(Operand, TypeId)],
    ) -> Expr {
        if !self.cx.target.is_wasm32() {
            let t = self.operand(target);
            let v = via
                .as_ref()
                .map(|o| self.operand(o))
                .unwrap_or_else(|| Expr::i(0));
            let m = method
                .as_ref()
                .map(|o| self.operand(o))
                .unwrap_or_else(|| Expr::i(0));
            return Expr::call("dream_js_call", vec![t, v, m, Expr::i(args.len() as i64)]);
        }
        let t = self.operand(target);
        let v = via.as_ref().map(|o| self.operand(o));
        let meth = method.as_ref().map(|o| self.operand(o));
        let mut filled: Vec<(i32, i32, &'static str, Expr)> = Vec::with_capacity(args.len());
        for (op, ty) in args {
            let (tag, aux, store) = js_abi::slot_desc(self.cx.interner, *ty);
            let mut payload = self.operand(op);
            if tag == js_abi::tag::FUNC {
                payload = Expr::call("dream_funcbox_funcidx", vec![payload]);
            }
            filled.push((tag, aux, store, payload));
        }
        let name = self.cx.callee_c(callee.def, &callee.args);
        let argc = args.len();
        self.b.expr_block(move |b| {
            let mut host_args = vec![t];
            if let Some(p) = v {
                host_args.push(p);
            }
            if let Some(n) = meth {
                host_args.push(n);
            }
            let slots_ptr = if argc == 0 {
                Expr::i(0)
            } else {
                let nbytes = argc as u32 * js_abi::SLOT_SIZE;
                let buf = b.temp(
                    CTy::Array {
                        elem: Box::new(CTy::U8),
                        len: nbytes as usize,
                    },
                    None,
                );
                b.call(
                    "memset",
                    vec![
                        Expr::addr_of(buf.clone()),
                        Expr::i(0),
                        Expr::i(nbytes as i64),
                    ],
                );
                let base = Expr::cast(
                    CTy::Ptr,
                    Expr::cast(CTy::Named("uintptr_t"), Expr::addr_of(buf)),
                );
                for (i, (tag, aux, store, payload)) in filled.into_iter().enumerate() {
                    let off = i as u32 * js_abi::SLOT_SIZE;
                    b.stmt(Stmt::store(
                        CTy::I32,
                        Expr::ptr_add(base.clone(), Expr::i(off as i64)),
                        Expr::i(tag as i64),
                    ));
                    b.stmt(Stmt::store(
                        CTy::I32,
                        Expr::ptr_add(
                            base.clone(),
                            Expr::i((off + js_abi::SLOT_AUX_OFFSET) as i64),
                        ),
                        Expr::i(aux as i64),
                    ));
                    let pay = Expr::ptr_add(
                        base.clone(),
                        Expr::i((off + js_abi::SLOT_PAYLOAD_OFFSET) as i64),
                    );
                    match store {
                        "i64.store" => b.stmt(Stmt::store(CTy::I64, pay, payload)),
                        "f64.store" => b.stmt(Stmt::store(CTy::F64, pay, Expr::cast(CTy::F64, payload))),
                        "f32.store" => b.stmt(Stmt::store(CTy::F32, pay, payload)),
                        _ => b.stmt(Stmt::store(CTy::I32, pay, payload)),
                    }
                }
                base
            };
            host_args.push(Expr::cast(CTy::I32, slots_ptr));
            host_args.push(Expr::i(argc as i64));
            Expr::call(name, host_args)
        })
    }
}

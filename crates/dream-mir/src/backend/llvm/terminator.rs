//! Block terminators, including the async ones (only reachable inside a poll function).

use super::fx::{Fx, V};
use super::ir::{Ty, Value};
use crate::backend::shared::abi_types::elem_size;
use crate::backend::shared::place_policy::teardown_value_locals;
use crate::{Local, Operand, Place, Terminator};
use dream_types::{PrimTy, TyKind};

impl<'l, 'a> Fx<'l, 'a> {
    fn block_ref(&self, b: crate::BlockId) -> super::ir::BlockRef {
        self.blocks[b.0 as usize]
    }

    fn self_ref(&self) -> V {
        V::u(
            self.self_.clone().unwrap_or_else(|| {
                crate::internal_error!("async terminator outside a poll function")
            }),
        )
    }

    fn value_local_of(&self, o: &Operand) -> Option<Local> {
        match o {
            Operand::Copy(Place::Local(l)) if self.is_value(self.f.local_ty(*l)) => Some(*l),
            _ => None,
        }
    }

    pub fn term(&mut self, t: &Terminator) {
        match t {
            Terminator::Goto(b) => {
                let b = self.block_ref(*b);
                self.w.br(b);
            }
            Terminator::If {
                cond,
                then_blk,
                else_blk,
            } => {
                let c = self.operand(cond);
                let c = self.truthy(&c);
                let (t, e) = (self.block_ref(*then_blk), self.block_ref(*else_blk));
                self.w.cond_br(&c, t, e);
            }
            Terminator::Switch {
                value,
                targets,
                default,
            } => {
                let v = self.operand(value);
                let v = self.conv(&v, &Ty::I64);
                let mut arms: Vec<(i128, super::ir::BlockRef)> = Vec::new();
                for (k, b) in targets {
                    if !arms.iter().any(|(x, _)| *x == *k as i128) {
                        arms.push((*k as i128, self.block_ref(*b)));
                    }
                }
                let d = self.block_ref(*default);
                self.w.switch(&v, d, &arms);
            }
            Terminator::Return(None) => {
                if !self.f.is_async {
                    self.value_teardown(None);
                }
                self.ret_default();
            }
            Terminator::Return(Some(o)) => {
                if !self.f.is_async {
                    self.value_teardown(self.value_local_of(o));
                }
                if self.l.sret.contains(&self.l.user_fn(self.f)) {
                    let size = elem_size(&self.l.cx, self.f.ret) as i64;
                    let src = self.operand(o);
                    let buf = self.w.param(self.w.param_count() - 1);
                    let sp = self.ptr(&src);
                    self.memcpy(&buf, &sp, &Value::i64(size));
                    self.w.ret(None);
                } else if !self.f.is_async && self.is_value(self.f.ret) {
                    let size = elem_size(&self.l.cx, self.f.ret) as i64;
                    let tag = self.l.cx.type_tag(self.f.ret);
                    let src = self.operand(o);
                    let r = self.call_v("dream_malloc", &[V::i32(size), V::i32(tag as i64)]);
                    let (rp, sp) = (self.ptr(&r), self.ptr(&src));
                    self.memcpy(&rp, &sp, &Value::i64(size));
                    self.ret_v(&r);
                } else {
                    let v = self.operand(o);
                    self.ret_v(&v);
                }
            }
            Terminator::Unreachable => {
                self.call("abort", &[]);
                if !self.w.is_terminated() {
                    self.w.unreachable();
                }
            }
            Terminator::TailCall { callee, args } => {
                if !self.f.is_async {
                    self.value_teardown(None);
                }
                let r = self.call_expr(callee, args);
                match (r, matches!(self.interner.kind(self.f.ret), TyKind::Void)) {
                    (Some(v), false) => self.ret_v(&v),
                    _ => self.ret_default(),
                }
            }
            Terminator::AsyncComplete(None) => {
                self.value_teardown(None);
                let s = self.self_ref();
                self.call("dream_async_complete", &[s, V::i32(0)]);
                self.w.ret(Some(&Value::i32(0)));
            }
            Terminator::AsyncComplete(Some(o)) => {
                self.value_teardown(self.value_local_of(o));
                let result = self.operand(o);
                let s = self.self_ref();
                let wide = self.l.cx.target.abi().future.wide as i64;
                let wide_ty = match self.interner.kind(self.f.ret) {
                    TyKind::Prim(PrimTy::Long | PrimTy::ULong) => Some(Ty::I64),
                    TyKind::Prim(PrimTy::Float) => Some(Ty::F32),
                    TyKind::Prim(PrimTy::Double) => Some(Ty::F64),
                    _ => None,
                };
                match wide_ty {
                    Some(t) => {
                        let at = self.addr(&s, wide);
                        self.store_ty(&t, &at, &result, 8);
                        self.call("dream_async_complete", &[s, V::i32(0)]);
                    }
                    None => {
                        let r = self.as_ref(&result);
                        self.call("dream_async_complete", &[s, r]);
                    }
                }
                self.w.ret(Some(&Value::i32(0)));
            }
            Terminator::Await {
                future,
                dest: _,
                resume,
            } => {
                let fut = self.operand(future);
                let s = self.self_ref();
                let state = self.l.cx.target.abi().future.state as i64;
                let at = self.addr(&s, state);
                self.store_ty(&Ty::I32, &at, &V::i32(resume.0 as i64), 4);
                self.call("dream_await", &[s, fut]);
                self.w.ret(Some(&Value::i32(0)));
            }
        }
    }

    fn ret_v(&mut self, v: &V) {
        let t = self.w.ret.clone();
        if t.is_void() {
            self.w.ret(None);
            return;
        }
        let r = self.conv(v, &t);
        self.w.ret(Some(&r));
    }

    /// `return;` — a non-void function (async stubs, `main`) answers zero.
    pub fn ret_default(&mut self) {
        let t = self.w.ret.clone();
        if t.is_void() {
            self.w.ret(None);
        } else {
            self.w.ret(Some(&Value::zero(t)));
        }
    }

    fn value_teardown(&mut self, skip: Option<Local>) {
        let locals = teardown_value_locals(self.interner, self.f, skip);
        for local in locals {
            let v = self.read_local(local);
            self.value_refs(self.f.local_ty(local), &v, false);
        }
    }
}

use super::ast::{CTy, CaseKey, Expr, Stmt, SwitchArm};
use super::emit::Emitter;
use super::places::{is_alias_value_local, is_moved_into_union, is_value_copy_local};
use crate::{BlockId, Local, Operand, Place, Terminator};
use dream_types::TyKind;

impl<'a> Emitter<'a> {
    pub(super) fn term(&mut self, t: &Terminator) {
        match t {
            Terminator::Goto(b) => self.b.goto(format!("L{}", b.0)),
            Terminator::If {
                cond,
                then_blk,
                else_blk,
            } => {
                let c = self.operand(cond);
                self.b.stmt(Stmt::if_else(
                    c,
                    Stmt::Goto(format!("L{}", then_blk.0)),
                    Stmt::Goto(format!("L{}", else_blk.0)),
                ));
            }
            Terminator::Switch {
                value,
                targets,
                default,
            } => self.switch(value, targets, *default),
            Terminator::Return(None) => {
                self.value_teardown(None);
                if self.f.is_async || !matches!(self.cx.interner.kind(self.f.ret), TyKind::Void) {
                    self.b.ret(Some(Expr::i(0)));
                } else {
                    self.b.ret(None);
                }
            }
            Terminator::Return(Some(o)) => {
                let skip = match o {
                    Operand::Copy(Place::Local(l))
                        if self.cx.interner.is_value_type(self.f.local_ty(*l)) =>
                    {
                        Some(*l)
                    }
                    _ => None,
                };
                self.value_teardown(skip);
                if !self.f.is_async && self.cx.interner.is_value_type(self.f.ret) {
                    let size = super::types::elem_size(self.cx, self.f.ret);
                    let tag = self.cx.type_tag(self.f.ret, dream_types::DefId(0));
                    let src = self.operand(o);
                    self.b.stmt(Stmt::block(vec![
                        Stmt::decl(
                            CTy::Ptr,
                            "__r",
                            Some(Expr::call(
                                "dream_malloc",
                                vec![Expr::i(size as i64), Expr::i(tag as i64)],
                            )),
                        ),
                        Stmt::call(
                            "memcpy",
                            vec![
                                Expr::dream_p(Expr::id("__r")),
                                Expr::dream_p(src),
                                Expr::i(size as i64),
                            ],
                        ),
                        Stmt::Return(Some(Expr::id("__r"))),
                    ]));
                } else {
                    let v = self.operand(o);
                    self.b.ret(Some(v));
                }
            }
            Terminator::Unreachable => self.b.call("abort", vec![]),
            Terminator::TailCall { callee, args } => {
                self.value_teardown(None);
                let call = self.call_expr(callee, args);
                if matches!(self.cx.interner.kind(self.f.ret), TyKind::Void) {
                    self.b.expr_stmt(call);
                    self.b.ret(None);
                } else {
                    self.b.ret(Some(call));
                }
            }
            Terminator::AsyncComplete(None) => {
                self.b
                    .call("dream_async_complete", vec![Expr::id("__self"), Expr::i(0)]);
                self.b.ret(Some(Expr::i(0)));
            }
            Terminator::AsyncComplete(Some(o)) => {
                let result = self.operand(o);
                let wide = crate::abi::FutureLayout::native().wide as i64;
                match self.cx.interner.kind(self.f.ret) {
                    TyKind::Prim(dream_types::PrimTy::Long | dream_types::PrimTy::ULong) => {
                        self.b.stmt(Stmt::store(
                            CTy::I64,
                            Expr::ptr_add(Expr::id("__self"), Expr::i(wide)),
                            result,
                        ));
                        self.b
                            .call("dream_async_complete", vec![Expr::id("__self"), Expr::i(0)]);
                        self.b.ret(Some(Expr::i(0)));
                    }
                    TyKind::Prim(dream_types::PrimTy::Float) => {
                        self.b.stmt(Stmt::store(
                            CTy::F32,
                            Expr::ptr_add(Expr::id("__self"), Expr::i(wide)),
                            result,
                        ));
                        self.b
                            .call("dream_async_complete", vec![Expr::id("__self"), Expr::i(0)]);
                        self.b.ret(Some(Expr::i(0)));
                    }
                    TyKind::Prim(dream_types::PrimTy::Double) => {
                        self.b.stmt(Stmt::store(
                            CTy::F64,
                            Expr::ptr_add(Expr::id("__self"), Expr::i(wide)),
                            result,
                        ));
                        self.b
                            .call("dream_async_complete", vec![Expr::id("__self"), Expr::i(0)]);
                        self.b.ret(Some(Expr::i(0)));
                    }
                    _ => {
                        self.b.call(
                            "dream_async_complete",
                            vec![Expr::id("__self"), Expr::cast(CTy::Ptr, result)],
                        );
                        self.b.ret(Some(Expr::i(0)));
                    }
                }
            }
            Terminator::Await {
                future,
                dest: _,
                resume,
            } => {
                let fut = self.operand(future);
                self.b.stmt(Stmt::store(
                    CTy::I32,
                    Expr::ptr_add(
                        Expr::id("__self"),
                        Expr::i(crate::abi::FutureLayout::native().state as i64),
                    ),
                    Expr::i(resume.0 as i64),
                ));
                self.b.call("dream_await", vec![Expr::id("__self"), fut]);
                self.b.ret(Some(Expr::i(0)));
            }
        }
    }

    fn switch(&mut self, value: &Operand, targets: &[(i64, BlockId)], default: BlockId) {
        let v = self.operand(value);
        let mut keys: Vec<i64> = targets.iter().map(|(k, _)| *k).collect();
        keys.sort_unstable();
        let dense = !keys.is_empty()
            && keys[0] == 0
            && keys.windows(2).all(|w| w[1] == w[0] + 1)
            && keys.len() >= 2;
        if dense {
            let max = keys.last().copied().unwrap() as usize;
            let mut map = vec![default; max + 1];
            for (k, b) in targets {
                map[*k as usize] = *b;
            }
            let labels: Vec<Expr> = map
                .iter()
                .map(|b| Expr::LabelAddr(format!("L{}", b.0)))
                .collect();
            let n = map.len();
            self.b.stmt(Stmt::block(vec![
                Stmt::Decl {
                    align: None,
                    static_: true,
                    const_: true,
                    ty: CTy::Array {
                        elem: Box::new(CTy::VoidPtr),
                        len: n,
                    },
                    name: "__jt".into(),
                    init: Some(Expr::Compound(labels)),
                },
                Stmt::decl(CTy::Unsigned, "__k", Some(Expr::cast(CTy::Unsigned, v))),
                Stmt::if_else(
                    Expr::lt(Expr::id("__k"), Expr::i(n as i64)),
                    Stmt::GotoIndirect(Expr::index(Expr::id("__jt"), Expr::id("__k"))),
                    Stmt::Goto(format!("L{}", default.0)),
                ),
            ]));
            return;
        }
        let mut arms = Vec::new();
        for (k, b) in targets {
            arms.push(SwitchArm {
                keys: vec![CaseKey::Int(*k)],
                body: vec![Stmt::Goto(format!("L{}", b.0))],
            });
        }
        arms.push(SwitchArm {
            keys: vec![],
            body: vec![Stmt::Goto(format!("L{}", default.0))],
        });
        self.b.stmt(Stmt::Switch {
            expr: Expr::cast(CTy::I64, v),
            arms,
        });
    }

    fn value_teardown(&mut self, skip: Option<Local>) {
        if self.f.is_async
            || self
                .f
                .blocks
                .iter()
                .any(|b| matches!(b.terminator, Terminator::Await { .. }))
        {
            return;
        }
        let dropped: Vec<bool> = {
            let mut d = vec![false; self.f.locals.len()];
            for stmt in self.f.blocks.iter().flat_map(|block| &block.stmts) {
                if let crate::Statement::ValueDrop(l) = stmt {
                    if !self.f.locals[l.0 as usize].is_ref {
                        d[l.0 as usize] = true;
                    }
                }
            }
            d
        };
        for (i, decl) in self.f.locals.iter().enumerate() {
            let local = Local(i as u32);
            if skip == Some(local)
                || decl.manual_drop
                || decl.is_ref
                || dropped[i]
                || is_alias_value_local(self.f, local)
                || is_value_copy_local(self.f, local)
                || is_moved_into_union(self.f, local)
            {
                continue;
            }
            if !self.cx.interner.is_value_type(decl.ty) {
                continue;
            }
            if self.f.params.iter().any(|p| p.0 == local.0)
                && (decl.is_ref || decl.name.as_deref() == Some("this"))
            {
                continue;
            }
            self.value_refs(decl.ty, Expr::local(i as u32), false);
        }
    }
}

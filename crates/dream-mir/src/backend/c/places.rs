use super::ast::{CTy, Expr, Stmt, UnOp};
use super::emit::Emitter;
use super::types::{array_elem_ty, elem_size, load_cast, local_c_ty};
use crate::{Operand, Place};

impl<'a> Emitter<'a> {
    pub(super) fn operand(&mut self, o: &Operand) -> Expr {
        match o {
            Operand::Copy(p) => self.load(p),
            Operand::Const(c) => match c {
                crate::Const::Int(v) => Expr::i(*v),
                crate::Const::Long(v) => Expr::Long(*v),
                crate::Const::Float(v) => {
                    if v.is_nan() {
                        Expr::Nan { double: true }
                    } else if v.is_infinite() {
                        Expr::Inf {
                            double: true,
                            neg: *v < 0.0,
                        }
                    } else {
                        Expr::Float(*v)
                    }
                }
                crate::Const::F32(v) => {
                    if v.is_nan() {
                        Expr::Nan { double: false }
                    } else if v.is_infinite() {
                        Expr::Inf {
                            double: false,
                            neg: *v < 0.0,
                        }
                    } else {
                        Expr::F32(*v)
                    }
                }
                crate::Const::Bool(b) => Expr::i(if *b { 1 } else { 0 }),
                crate::Const::Char(ch) => Expr::i(*ch as i64),
                crate::Const::Str(s) => Expr::id(self.cx.str_sym(s)),
                crate::Const::Null => Expr::Null,
            },
        }
    }

    pub(super) fn load(&mut self, place: &Place) -> Expr {
        match place {
            Place::Local(l) => Expr::local(l.0),
            Place::Global(g) => Expr::global(g.0),
            Place::Field { base, field } => {
                let ty = self.f.local_ty(*base);
                let Some(layout) = self.cx.nstruct(ty) else {
                    return Expr::index(
                        Expr::cast(CTy::ptr_to(CTy::Ptr), Expr::local(base.0)),
                        Expr::i(*field as i64),
                    );
                };
                let fld = layout.fields.get(*field).unwrap_or_else(|| {
                    crate::internal_error!("missing field {field} on type {ty:?}")
                });
                if self.cx.interner.is_value_type(fld.ty) {
                    return Expr::cast(CTy::Ptr, Expr::field_ptr(base.0, fld.offset));
                }
                let cast = load_cast(self.cx, fld.ty);
                let load = Expr::load(cast.clone(), Expr::field_ptr(base.0, fld.offset));
                if fld.is_unowned {
                    let panic = Expr::id(
                        self.cx
                            .str_sym(crate::backend::shared::panic_msgs::UNOWNED_NULL_DEREF),
                    );
                    self.b.expr_block(|b| {
                        let t = b.temp(cast.clone(), Some(load.clone()));
                        b.stmt(Stmt::if_(
                            Expr::unary(UnOp::Not, t.clone()),
                            Stmt::call("dream_panic", vec![panic.clone()]),
                        ));
                        t
                    })
                } else {
                    load
                }
            }
            Place::Index {
                base,
                index,
                unchecked,
            } => {
                let ety = array_elem_ty(self.cx.interner, self.f.local_ty(*base));
                let addr = self.index_addr(*base, index, elem_size(self.cx, ety), *unchecked);
                if self.cx.interner.is_value_type(ety) {
                    Expr::cast(CTy::Ptr, addr)
                } else {
                    Expr::load(load_cast(self.cx, ety), addr)
                }
            }
            Place::Deref { ptr, elem_ty } => {
                if self.cx.interner.is_value_type(*elem_ty) {
                    Expr::local(ptr.0)
                } else {
                    Expr::load(
                        load_cast(self.cx, *elem_ty),
                        Expr::dream_p(Expr::local(ptr.0)),
                    )
                }
            }
        }
    }

    fn bind_rhs(&mut self, ty: CTy, rhs: Expr) -> Expr {
        if rhs.is_dup_safe() {
            rhs
        } else {
            self.b.temp(ty, Some(rhs))
        }
    }

    fn store_val_ty(&self, place: &Place) -> CTy {
        let value_ptr = |ty| {
            if self.cx.interner.is_value_type(ty) {
                CTy::Ptr
            } else {
                load_cast(self.cx, ty)
            }
        };
        match place {
            // Locals are register-width (`int` is i64, `char` is i32). `load_cast` is the
            // in-memory packed layout and would truncate UTF-16 chars and native pointers.
            Place::Local(l) => {
                let ty = self.f.local_ty(*l);
                if self.cx.interner.is_value_type(ty) {
                    CTy::Ptr
                } else {
                    local_c_ty(self.cx.interner, ty)
                }
            }
            Place::Global(g) => {
                if g.0 == 0 {
                    CTy::Ptr
                } else {
                    self.cx
                        .mir
                        .globals
                        .iter()
                        .find(|global| global.id == *g)
                        .map(|global| value_ptr(global.ty))
                        .unwrap_or(CTy::Ptr)
                }
            }
            Place::Field { base, field } => self
                .cx
                .nstruct(self.f.local_ty(*base))
                .and_then(|layout| layout.fields.get(*field))
                .map(|fld| value_ptr(fld.ty))
                .unwrap_or(CTy::Ptr),
            Place::Index { base, .. } => {
                value_ptr(array_elem_ty(self.cx.interner, self.f.local_ty(*base)))
            }
            Place::Deref { elem_ty, .. } => value_ptr(*elem_ty),
        }
    }

    pub(super) fn store(&mut self, place: &Place, rv: &crate::Rvalue, rhs: Expr) -> Expr {
        let rhs = self.bind_rhs(self.store_val_ty(place), rhs);
        match place {
            Place::Local(l) if self.cx.interner.is_value_type(self.f.local_ty(*l)) => {
                if is_value_place_alias(self.f, *l, rv) {
                    return self.b.expr_block(|b| {
                        b.assign(Expr::local(l.0), Expr::cast(CTy::Ptr, rhs.clone()));
                        Expr::local(l.0)
                    });
                }
                if let crate::Rvalue::Use(Operand::Copy(Place::Local(src))) = rv {
                    let src_ty = self.f.local_ty(*src);
                    if !self.cx.interner.is_value_type(src_ty)
                        && self.cx.nstruct(src_ty).is_some_and(|layout| {
                            layout.size == elem_size(self.cx, self.f.local_ty(*l))
                        })
                    {
                        return self.b.expr_block(|b| {
                            b.assign(Expr::local(l.0), Expr::local(src.0));
                            Expr::local(l.0)
                        });
                    }
                }
                let retain_copy = matches!(
                    rv,
                    crate::Rvalue::Use(Operand::Copy(Place::Field { .. }))
                        | crate::Rvalue::Use(Operand::Copy(Place::Index { .. }))
                        | crate::Rvalue::Use(Operand::Copy(Place::Deref { .. }))
                        | crate::Rvalue::UnionField { .. }
                );
                self.memcpy_value(
                    rv,
                    rhs,
                    self.f.local_ty(*l),
                    Expr::dream_p(Expr::local(l.0)),
                    Expr::local(l.0),
                    retain_copy,
                )
            }
            Place::Local(l) => self.b.expr_block(|b| {
                b.assign(Expr::local(l.0), rhs.clone());
                Expr::local(l.0)
            }),
            Place::Global(g) => {
                let value_ty = self
                    .cx
                    .mir
                    .globals
                    .iter()
                    .find(|global| global.id == *g)
                    .map(|global| global.ty)
                    .filter(|ty| self.cx.interner.is_value_type(*ty));
                if let Some(ty) = value_ty {
                    let size = elem_size(self.cx, ty);
                    self.b.expr_block(|b| {
                        b.call(
                            "memcpy",
                            vec![
                                Expr::dream_p(Expr::global(g.0)),
                                Expr::dream_p(rhs.clone()),
                                Expr::i(size as i64),
                            ],
                        );
                        Expr::global(g.0)
                    })
                } else {
                    self.b.expr_block(|b| {
                        b.assign(Expr::global(g.0), rhs.clone());
                        Expr::global(g.0)
                    })
                }
            }
            Place::Field { base, field } => {
                let ty = self.f.local_ty(*base);
                let Some(layout) = self.cx.nstruct(ty) else {
                    let dest = Expr::index(
                        Expr::cast(CTy::ptr_to(CTy::Ptr), Expr::local(base.0)),
                        Expr::i(*field as i64),
                    );
                    return self.b.expr_block(|b| {
                        b.assign(dest.clone(), Expr::cast(CTy::Ptr, rhs.clone()));
                        dest
                    });
                };
                let fld = layout.fields.get(*field).unwrap_or_else(|| {
                    crate::internal_error!("missing field {field} on type {ty:?}")
                });
                if self.cx.interner.is_value_type(fld.ty) {
                    return self.memcpy_value(
                        rv,
                        rhs,
                        fld.ty,
                        Expr::field_ptr(base.0, fld.offset),
                        Expr::cast(CTy::Ptr, Expr::field_ptr(base.0, fld.offset)),
                        true,
                    );
                }
                let cast = load_cast(self.cx, fld.ty);
                let slot = Expr::field_ptr(base.0, fld.offset);
                if realloc_self_store(place, rv) {
                    return self.b.expr_block(|b| {
                        b.stmt(Stmt::store(cast.clone(), slot.clone(), rhs.clone()));
                        rhs.clone()
                    });
                }
                if fld.is_unowned {
                    return self.unowned_store(slot, rhs);
                }
                if fld.is_weak {
                    return self.weak_option_store(*base, fld, rv, rhs);
                }
                if self.cx.interner.is_reference(fld.ty) && !fld.is_weak {
                    let release = crate::backend::c::release::release_sym(
                        self.cx.interner,
                        self.cx.mir,
                        fld.ty,
                    );
                    return self.rc_store(cast, slot, rhs, release, borrowed_ref_store(rv));
                }
                self.b.expr_block(|b| {
                    b.stmt(Stmt::store(cast.clone(), slot.clone(), rhs.clone()));
                    rhs.clone()
                })
            }
            Place::Index {
                base,
                index,
                unchecked,
            } => {
                let ety = array_elem_ty(self.cx.interner, self.f.local_ty(*base));
                let es = elem_size(self.cx, ety);
                let addr = self.index_addr(*base, index, es, *unchecked);
                if self.cx.interner.is_value_type(ety) {
                    return self.b.expr_block(|b| {
                        b.call(
                            "memcpy",
                            vec![addr.clone(), Expr::dream_p(rhs.clone()), Expr::i(es as i64)],
                        );
                        rhs.clone()
                    });
                }
                let cast = load_cast(self.cx, ety);
                if realloc_self_store(place, rv) {
                    return self.b.expr_block(|b| {
                        b.stmt(Stmt::store(cast.clone(), addr.clone(), rhs.clone()));
                        rhs.clone()
                    });
                }
                if self.cx.interner.is_reference(ety) {
                    let release =
                        crate::backend::c::release::release_sym(self.cx.interner, self.cx.mir, ety);
                    return self.rc_store(cast, addr, rhs, release, borrowed_ref_store(rv));
                }
                self.b.expr_block(|b| {
                    b.stmt(Stmt::store(cast.clone(), addr.clone(), rhs.clone()));
                    rhs.clone()
                })
            }
            Place::Deref { ptr, elem_ty } => {
                if self.cx.interner.is_value_type(*elem_ty) {
                    let size = elem_size(self.cx, *elem_ty);
                    return self.b.expr_block(|b| {
                        b.call(
                            "memcpy",
                            vec![
                                Expr::dream_p(Expr::local(ptr.0)),
                                Expr::dream_p(rhs.clone()),
                                Expr::i(size as i64),
                            ],
                        );
                        rhs.clone()
                    });
                }
                let cast = load_cast(self.cx, *elem_ty);
                self.b.expr_block(|b| {
                    b.stmt(Stmt::store(
                        cast.clone(),
                        Expr::dream_p(Expr::local(ptr.0)),
                        rhs.clone(),
                    ));
                    rhs.clone()
                })
            }
        }
    }

    fn unowned_store(&mut self, slot: Expr, rhs: Expr) -> Expr {
        self.b.expr_block(|b| {
            let old = b.temp(CTy::Ptr, Some(Expr::load(CTy::Ptr, slot.clone())));
            b.stmt(Stmt::if_(
                old.clone(),
                Stmt::call(
                    "dream_weak_unregister",
                    vec![old.clone(), Expr::cast(CTy::Ptr, slot.clone())],
                ),
            ));
            let new = b.temp(CTy::Ptr, Some(Expr::cast(CTy::Ptr, rhs.clone())));
            b.stmt(Stmt::store(CTy::Ptr, slot.clone(), new.clone()));
            b.stmt(Stmt::if_(
                new.clone(),
                Stmt::call(
                    "dream_weak_register",
                    vec![
                        new.clone(),
                        Expr::cast(CTy::Ptr, slot.clone()),
                        Expr::i(1),
                        Expr::i(0),
                    ],
                ),
            ));
            new
        })
    }

    fn rc_store(
        &mut self,
        cast: CTy,
        slot: Expr,
        rhs: Expr,
        release: String,
        borrowed: bool,
    ) -> Expr {
        self.b.expr_block(|b| {
            let old = b.temp(CTy::Ptr, Some(Expr::load(CTy::Ptr, slot.clone())));
            let v = b.temp(CTy::Ptr, Some(Expr::cast(CTy::Ptr, rhs.clone())));
            if borrowed {
                b.stmt(Stmt::if_(
                    Expr::ne(old.clone(), v.clone()),
                    Stmt::block(vec![
                        Stmt::call("dream_retain", vec![v.clone()]),
                        Stmt::store(cast.clone(), slot.clone(), v.clone()),
                        Stmt::call(release.clone(), vec![old.clone()]),
                    ]),
                ));
            } else {
                b.stmt(Stmt::store(cast.clone(), slot.clone(), v.clone()));
                b.call(release.clone(), vec![old.clone()]);
            }
            v
        })
    }

    fn index_addr(
        &mut self,
        base: crate::Local,
        index: &Operand,
        elem_size: u32,
        unchecked: bool,
    ) -> Expr {
        let idx = self.operand(index);
        if unchecked
            || !matches!(
                self.cx.interner.kind(self.f.local_ty(base)),
                dream_types::TyKind::Array(_)
            )
        {
            return Expr::add(
                Expr::ptr_add(Expr::local(base.0), super::types::len_prefix()),
                Expr::mul(idx, Expr::i(elem_size as i64)),
            );
        }
        let panic = Expr::id(
            self.cx
                .str_sym(crate::backend::shared::panic_msgs::INDEX_OUT_OF_BOUNDS),
        );
        let idx_e = idx.clone();
        self.b.expr_block(move |b| {
            let t_idx = b.temp(CTy::I32, Some(Expr::cast(CTy::I32, idx_e.clone())));
            let t_len = b.temp(
                CTy::I32,
                Some(Expr::ternary(
                    Expr::local(base.0),
                    Expr::load(CTy::I32, Expr::dream_p(Expr::local(base.0))),
                    Expr::i(0),
                )),
            );
            let in_i32 = Expr::eq(
                Expr::cast(CTy::I64, idx_e.clone()),
                Expr::cast(CTy::I64, t_idx.clone()),
            );
            let oob = Expr::ge(
                Expr::cast(CTy::U32, t_idx.clone()),
                Expr::cast(CTy::U32, t_len),
            );
            b.stmt(Stmt::if_(
                Expr::and(in_i32, oob),
                Stmt::call("dream_panic", vec![panic.clone()]),
            ));
            Expr::add(
                Expr::ptr_add(Expr::local(base.0), super::types::len_prefix()),
                Expr::mul(
                    Expr::cast(CTy::I64, idx_e.clone()),
                    Expr::i(elem_size as i64),
                ),
            )
        })
    }

    fn memcpy_value(
        &mut self,
        rv: &crate::Rvalue,
        rhs: Expr,
        ty: dream_types::TypeId,
        dest: Expr,
        dest_ptr: Expr,
        retain_copy: bool,
    ) -> Expr {
        let size = elem_size(self.cx, ty);
        if value_rvalue_allocates(rv) {
            self.b.expr_block(move |b| {
                let v = b.temp(CTy::Ptr, Some(rhs.clone()));
                b.call(
                    "memcpy",
                    vec![dest.clone(), Expr::dream_p(v.clone()), Expr::i(size as i64)],
                );
                b.call("dream_free", vec![v.clone()]);
                dest_ptr.clone()
            })
        } else {
            let cx = self.cx;
            let func = self.f;
            self.b.expr_block(|b| {
                b.call(
                    "memcpy",
                    vec![
                        dest.clone(),
                        Expr::dream_p(rhs.clone()),
                        Expr::i(size as i64),
                    ],
                );
                if retain_copy {
                    let mut e = Emitter::new(cx, func, b);
                    e.value_refs(ty, dest_ptr.clone(), true);
                }
                dest_ptr.clone()
            })
        }
    }

    fn weak_option_store(
        &mut self,
        base: crate::Local,
        fld: &dream_hir::FieldLayout,
        rv: &crate::Rvalue,
        rhs: Expr,
    ) -> Expr {
        let Some(u) = self.cx.nunion(fld.ty) else {
            let slot = Expr::field_ptr(base.0, fld.offset);
            return self.b.expr_block(|b| {
                b.stmt(Stmt::store(CTy::Ptr, slot.clone(), rhs.clone()));
                rhs.clone()
            });
        };
        let some = u.variant("Some").map(|v| v.discriminant).unwrap_or(0);
        let none = u.variant("None").map(|v| v.discriminant).unwrap_or(1);
        let poff = u
            .variant("Some")
            .and_then(|v| v.fields.first())
            .map(|f| f.offset)
            .unwrap_or(8);
        let size = u.size.max(16);
        let slot = Expr::field_ptr(base.0, fld.offset);
        let drop_src = if value_rvalue_allocates(rv) {
            Some(crate::backend::c::release::release_sym(
                self.cx.interner,
                self.cx.mir,
                fld.ty,
            ))
        } else {
            None
        };
        self.b.expr_block(|b| {
            let src = b.temp(CTy::Ptr, Some(Expr::cast(CTy::Ptr, rhs.clone())));
            let old = b.temp(CTy::Ptr, Some(Expr::load(CTy::Ptr, slot.clone())));
            let box_ = b.temp(
                CTy::Ptr,
                Some(Expr::call(
                    "dream_malloc",
                    vec![Expr::i(size as i64), Expr::i(0)],
                )),
            );
            b.call(
                "memcpy",
                vec![
                    Expr::dream_p(box_.clone()),
                    Expr::dream_p(src.clone()),
                    Expr::i(size as i64),
                ],
            );
            b.stmt(Stmt::if_(
                Expr::eq(
                    Expr::load(CTy::I32, Expr::dream_p(src.clone())),
                    Expr::i(some as i64),
                ),
                Stmt::call(
                    "dream_weak_register",
                    vec![
                        Expr::load(CTy::Ptr, Expr::ptr_add(src.clone(), Expr::i(poff as i64))),
                        box_.clone(),
                        Expr::i(0),
                        Expr::cast(
                            CTy::Ptr,
                            Expr::cast(CTy::Named("intptr_t"), Expr::i(none as i64)),
                        ),
                    ],
                ),
            ));
            b.stmt(Stmt::store(CTy::Ptr, slot.clone(), box_.clone()));
            if let Some(rel) = &drop_src {
                b.call(rel.clone(), vec![src.clone()]);
            }
            b.stmt(Stmt::if_(
                old.clone(),
                Stmt::block(vec![
                    Stmt::if_(
                        Expr::eq(
                            Expr::load(CTy::I32, Expr::dream_p(old.clone())),
                            Expr::i(some as i64),
                        ),
                        Stmt::call(
                            "dream_weak_unregister",
                            vec![
                                Expr::load(
                                    CTy::Ptr,
                                    Expr::ptr_add(old.clone(), Expr::i(poff as i64)),
                                ),
                                old.clone(),
                            ],
                        ),
                    ),
                    Stmt::call("dream_free", vec![old.clone()]),
                ]),
            ));
            box_
        })
    }
}

fn realloc_self_store(place: &Place, rv: &crate::Rvalue) -> bool {
    let crate::Rvalue::ArrayRealloc { array, .. } = rv else {
        return false;
    };
    let Operand::Copy(src) = array else {
        return false;
    };
    match (place, src) {
        (
            Place::Field {
                base: b1,
                field: f1,
            },
            Place::Field {
                base: b2,
                field: f2,
            },
        ) => b1 == b2 && f1 == f2,
        (Place::Local(l1), Place::Local(l2)) => l1 == l2,
        (Place::Global(g1), Place::Global(g2)) => g1 == g2,
        _ => false,
    }
}

fn borrowed_ref_store(rv: &crate::Rvalue) -> bool {
    !value_rvalue_allocates(rv)
}

fn value_rvalue_allocates(rv: &crate::Rvalue) -> bool {
    matches!(
        rv,
        crate::Rvalue::New { .. }
            | crate::Rvalue::Tuple { .. }
            | crate::Rvalue::UnionNew { .. }
            | crate::Rvalue::Call { .. }
            | crate::Rvalue::InterfaceCall { .. }
            | crate::Rvalue::IndirectCall { .. }
            | crate::Rvalue::ArrayLit { .. }
            | crate::Rvalue::ArrayNew { .. }
            | crate::Rvalue::ArrayRealloc { .. }
    )
}

pub(super) fn is_alias_value_local(f: &crate::MirFunction, local: crate::Local) -> bool {
    if f.locals[local.0 as usize].name.is_some() {
        return false;
    }
    let mut seen = false;
    for stmt in f.blocks.iter().flat_map(|block| &block.stmts) {
        let crate::Statement::Assign(crate::Place::Local(other), rv) = stmt else {
            continue;
        };
        if *other != local {
            continue;
        }
        seen = true;
        if !is_value_place_alias(f, local, rv) {
            return false;
        }
    }
    seen
}

pub(super) fn is_value_copy_local(f: &crate::MirFunction, local: crate::Local) -> bool {
    let mut seen = false;
    for stmt in f.blocks.iter().flat_map(|block| &block.stmts) {
        let crate::Statement::Assign(crate::Place::Local(other), rv) = stmt else {
            continue;
        };
        if *other != local {
            continue;
        }
        seen = true;
        if !matches!(rv, crate::Rvalue::Use(_)) {
            return false;
        }
    }
    seen
}

pub(super) fn is_moved_into_union(f: &crate::MirFunction, local: crate::Local) -> bool {
    f.blocks.iter().flat_map(|block| &block.stmts).any(|stmt| {
        let crate::Statement::Assign(_, rv) = stmt else {
            return false;
        };
        let crate::Rvalue::UnionNew { args, .. } = rv else {
            return false;
        };
        args.iter().any(
            |arg| matches!(arg, crate::Operand::Copy(crate::Place::Local(src)) if *src == local),
        )
    })
}

pub(super) fn is_value_place_alias(
    f: &crate::MirFunction,
    local: crate::Local,
    rv: &crate::Rvalue,
) -> bool {
    if f.locals[local.0 as usize].name.is_some() {
        return false;
    }
    let place_ok = matches!(
        rv,
        crate::Rvalue::Use(Operand::Copy(Place::Local(_)))
            | crate::Rvalue::Use(Operand::Copy(Place::Index { .. }))
            | crate::Rvalue::Use(Operand::Copy(Place::Field { .. }))
            | crate::Rvalue::Use(Operand::Copy(Place::Deref { .. }))
            | crate::Rvalue::UnionField { .. }
    );
    if !place_ok {
        return false;
    }
    f.blocks.iter().flat_map(|block| &block.stmts).all(|stmt| {
        !matches!(
            stmt,
            crate::Statement::Assign(Place::Local(other), _) if *other == local
        ) || matches!(
            stmt,
            crate::Statement::Assign(
                Place::Local(other),
                crate::Rvalue::Use(Operand::Copy(Place::Local(_)))
                    | crate::Rvalue::Use(Operand::Copy(Place::Index { .. }))
                    | crate::Rvalue::Use(Operand::Copy(Place::Field { .. }))
                    | crate::Rvalue::Use(Operand::Copy(Place::Deref { .. }))
                    | crate::Rvalue::UnionField { .. }
            ) if *other == local
        )
    })
}

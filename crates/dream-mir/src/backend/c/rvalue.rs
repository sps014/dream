use super::ast::{CTy, Expr, Stmt, UnOp};
use super::emit::Emitter;
use super::types::{c_ident, elem_size, load_cast, runtime_c_name};
use crate::{Rvalue, UnOp as MirUnOp};
use dream_types::{PrimTy, TyKind};

impl<'a> Emitter<'a> {
    pub(super) fn rvalue(&mut self, rv: &Rvalue) -> Expr {
        match rv {
            Rvalue::Use(o) => self.operand(o),
            Rvalue::Select {
                cond,
                then_val,
                else_val,
            } => Expr::ternary(
                self.operand(cond),
                self.operand(then_val),
                self.operand(else_val),
            ),
            Rvalue::Binary(op, a, b) => {
                if *op == crate::BinOp::Eq
                    && matches!(self.operand_kind(a), Some(TyKind::Prim(PrimTy::String)))
                {
                    return Expr::call("dream_string_eq", vec![self.operand(a), self.operand(b)]);
                }
                if matches!(op, crate::BinOp::Div | crate::BinOp::Rem) && self.is_integer_operand(a)
                {
                    let lhs = self.operand(a);
                    let rhs = self.operand(b);
                    let symbol = Expr::id(self.cx.str_sym("panic: attempt to divide by zero"));
                    return self.b.expr_block(move |b| {
                        let t = b.temp(CTy::I64, Some(Expr::cast(CTy::I64, rhs.clone())));
                        Expr::ternary(
                            Expr::eq(t.clone(), Expr::i(0)),
                            Expr::comma(
                                Expr::call("dream_panic", vec![symbol.clone()]),
                                Expr::i(0),
                            ),
                            Expr::bin(*op, lhs.clone(), t),
                        )
                    });
                }
                Expr::bin(*op, self.operand(a), self.operand(b))
            }
            Rvalue::Unary(MirUnOp::Neg, a) => Expr::unary(UnOp::Neg, self.operand(a)),
            Rvalue::Unary(MirUnOp::Not, a) => Expr::unary(UnOp::Not, self.operand(a)),
            Rvalue::Unary(MirUnOp::BitNot, a) => Expr::unary(UnOp::BitNot, self.operand(a)),
            Rvalue::StrLen(s) => Expr::call("dream_str_len", vec![self.operand(s)]),
            Rvalue::StrByteSize(s) => Expr::call("dream_str_byte_size", vec![self.operand(s)]),
            Rvalue::CharAt(s, i, _) => Expr::cast(
                CTy::I32,
                Expr::call(
                    "dream_char_at_u",
                    vec![self.operand(s), Expr::cast(CTy::I32, self.operand(i))],
                ),
            ),
            Rvalue::ByteAt(s, i, _) => Expr::cast(
                CTy::I32,
                Expr::call(
                    "dream_byte_at_u",
                    vec![self.operand(s), Expr::cast(CTy::I32, self.operand(i))],
                ),
            ),
            Rvalue::ArrayNew { elem_ty, len } => {
                let es = elem_size(self.cx, *elem_ty);
                Expr::call(
                    "dream_array_new",
                    vec![self.operand(len), Expr::i(es as i64)],
                )
            }
            Rvalue::HashCode(o) => {
                let ty = self.operand_ty(o);
                let value = self.operand(o);
                self.hash_code_expr(ty, value)
            }
            Rvalue::ToString(o) => {
                let ty = self.operand_ty(o);
                let conv = self.to_string_fn(ty);
                if conv.is_empty() {
                    self.operand(o)
                } else {
                    Expr::call(conv, vec![self.operand(o)])
                }
            }
            Rvalue::Concat(parts) => self.concat_parts(parts),
            Rvalue::ConcatInt {
                prefix,
                value,
                suffix,
            } => Expr::call(
                "dream_concat_str_int_str",
                vec![
                    self.operand(prefix),
                    Expr::cast(CTy::I32, self.operand(value)),
                    self.operand(suffix),
                ],
            ),
            Rvalue::EnumName { value, arms } => {
                let v = self.operand(value);
                let mut e = Expr::id(self.cx.str_sym(""));
                for (k, name) in arms.iter().rev() {
                    e = Expr::ternary(
                        Expr::eq(Expr::cast(CTy::I64, v.clone()), Expr::Long(*k)),
                        Expr::id(self.cx.str_sym(name)),
                        e,
                    );
                }
                e
            }
            Rvalue::Call { callee, args } => self.call_expr(callee, args),
            Rvalue::IndirectCall { target, args, sig } => {
                let call = self.indirect_expr(target, args, *sig);
                match self.cx.interner.kind(*sig) {
                    TyKind::Func(_, ret)
                        if matches!(
                            self.cx.interner.kind(*ret),
                            TyKind::Prim(
                                PrimTy::Int
                                    | PrimTy::UInt
                                    | PrimTy::Bool
                                    | PrimTy::Byte
                                    | PrimTy::Char
                            ) | TyKind::Enum(_)
                        ) =>
                    {
                        Expr::cast(CTy::I32, Expr::cast(CTy::Named("intptr_t"), call))
                    }
                    _ => call,
                }
            }
            Rvalue::InterfaceCall {
                receiver,
                iface_id,
                method_slot,
                sig,
                args,
                ..
            } => self.iface_expr(receiver, *iface_id, *method_slot, *sig, args),
            Rvalue::FuncRef(callee) => {
                let idx = self
                    .cx
                    .ft
                    .get(&(callee.def, callee.args.clone()))
                    .copied()
                    .unwrap_or(0);
                Expr::i(idx as i64)
            }
            Rvalue::New {
                def,
                ty,
                ctor,
                args,
            } => self.emit_new(*def, *ty, *ctor, args),
            Rvalue::Tuple { ty, elems } => self.emit_tuple(*ty, elems),
            Rvalue::UnionNew {
                def,
                ty,
                variant,
                args,
            } => self.emit_union_new(*def, *ty, *variant, args),
            Rvalue::ArrayLit { elem_ty, elems } => self.emit_array_lit(*elem_ty, elems),
            Rvalue::ArrayLen(a) => Expr::load(CTy::I32, Expr::dream_p(self.operand(a))),
            Rvalue::ToBytes { value, ty } => {
                let sz = elem_size(self.cx, *ty);
                let value = self.operand(value);
                match self.cx.interner.kind(*ty) {
                    TyKind::Prim(PrimTy::String) => {
                        Expr::call("dream_to_bytes", vec![value, Expr::i(sz as i64)])
                    }
                    TyKind::Prim(_) | TyKind::Enum(_) => {
                        let cast = load_cast(self.cx, *ty);
                        Expr::call(
                            "dream_to_bytes",
                            vec![
                                Expr::cast(
                                    CTy::Ptr,
                                    Expr::cast(
                                        CTy::Named("uintptr_t"),
                                        Expr::addr_of(Expr::CompoundTyped {
                                            ty: cast.clone(),
                                            elems: vec![Expr::cast(cast, value)],
                                        }),
                                    ),
                                ),
                                Expr::i(sz as i64),
                            ],
                        )
                    }
                    _ => Expr::call("dream_to_bytes", vec![value, Expr::i(sz as i64)]),
                }
            }
            Rvalue::FromBytes { bytes, ty } => {
                let tag = self.cx.type_tag(*ty, dream_types::DefId(0));
                let sz = elem_size(self.cx, *ty);
                let bytes = self.operand(bytes);
                let from_bytes = Expr::call(
                    "dream_from_bytes",
                    vec![bytes, Expr::i(sz as i64), Expr::i(tag as i64)],
                );
                match self.cx.interner.kind(*ty) {
                    TyKind::Prim(PrimTy::String) => from_bytes,
                    TyKind::Prim(_) | TyKind::Enum(_) => {
                        let cast = load_cast(self.cx, *ty);
                        Expr::load(cast, Expr::dream_p(from_bytes))
                    }
                    _ => from_bytes,
                }
            }
            Rvalue::ArrayRealloc {
                elem_ty,
                array,
                new_len,
            } => {
                let es = elem_size(self.cx, *elem_ty);
                let arr = self.operand(array);
                let len = self.operand(new_len);
                // RC-tracked elements must release dropped tail slots on shrink, or truncated
                // elements stay retained until their slots are overwritten (the plain realloc
                // just rewrites the length prefix).
                if self.cx.interner.is_rc_tracked(*elem_ty) {
                    let rel = super::release::release_sym(self.cx, *elem_ty);
                    return Expr::call(
                        "dream_array_realloc_rc",
                        vec![arr, len, Expr::i(es as i64), Expr::id(&rel)],
                    );
                }
                Expr::call(
                    "dream_array_realloc",
                    vec![arr, len, Expr::i(es as i64)],
                )
            }
            Rvalue::Cast(v, from, to) => self.emit_cast(v, *from, *to),
            Rvalue::Discriminant(o) => Expr::load(CTy::I32, Expr::dream_p(self.operand(o))),
            Rvalue::UnionField {
                base,
                ty,
                variant,
                field,
            } => {
                let u = self
                    .cx
                    .nunion(*ty)
                    .unwrap_or_else(|| crate::internal_error!("missing union layout for {ty:?}"));
                let var = u
                    .variants
                    .iter()
                    .find(|v| v.discriminant as usize == *variant)
                    .unwrap_or_else(|| crate::internal_error!("missing union variant {variant}"));
                let fld = var
                    .fields
                    .get(*field)
                    .unwrap_or_else(|| crate::internal_error!("missing union field {field}"));
                let base = self.operand(base);
                if self.cx.interner.is_value_type(fld.ty) {
                    Expr::cast(CTy::Ptr, Expr::ptr_add(base, Expr::i(fld.offset as i64)))
                } else {
                    let cast = load_cast(self.cx, fld.ty);
                    Expr::load(cast, Expr::ptr_add(base, Expr::i(fld.offset as i64)))
                }
            }
            Rvalue::IsType(o, ty) => {
                let tag = runtime_tag(self.cx, *ty);
                Expr::eq(
                    Expr::call("dream_object_tag", vec![self.operand(o)]),
                    Expr::i(tag as i64),
                )
            }
            Rvalue::JsCall {
                callee,
                target,
                via,
                method,
                args,
            } => self.js_call_expr(callee, target, via, method, args),
        }
    }

    fn operand_kind(&self, o: &crate::Operand) -> Option<&TyKind> {
        match o {
            crate::Operand::Copy(crate::Place::Local(l)) => {
                Some(self.cx.interner.kind(self.f.local_ty(*l)))
            }
            crate::Operand::Const(crate::Const::Str(_)) => {
                Some(self.cx.interner.kind(self.cx.interner.string()))
            }
            _ => None,
        }
    }

    fn is_integer_operand(&self, o: &crate::Operand) -> bool {
        if matches!(
            o,
            crate::Operand::Const(crate::Const::Int(_) | crate::Const::Long(_))
        ) {
            return true;
        }
        matches!(
            self.operand_kind(o),
            Some(TyKind::Prim(
                PrimTy::Int | PrimTy::UInt | PrimTy::Long | PrimTy::ULong | PrimTy::Byte
            )) | Some(TyKind::Enum(_))
        )
    }

    fn emit_new(
        &mut self,
        def: dream_types::DefId,
        ty: dream_types::TypeId,
        ctor: Option<dream_types::DefId>,
        args: &[crate::Operand],
    ) -> Expr {
        let layout = self.cx.nstruct(ty).unwrap_or_else(|| {
            crate::internal_error!("missing layout for struct allocation {ty:?}")
        });
        let mut size = layout.size;
        if self.cx.interner.is_shared_type(ty) {
            size += crate::abi::HEADER_LOCK_WORD_SIZE;
        }
        let tag = self.cx.type_tag(ty, def);
        for a in args {
            self.retain_rc_global_sink(true, a);
        }
        let arg_es: Vec<Expr> = args.iter().map(|a| self.operand(a)).collect();
        let ctor_name = ctor.map(|c| runtime_c_name(&self.cx.callee_c(c, &[])));
        self.b.expr_block(move |b| {
            let o = b.temp(
                CTy::Ptr,
                Some(Expr::call(
                    "dream_malloc",
                    vec![Expr::i(size as i64), Expr::i(tag as i64)],
                )),
            );
            b.call(
                "memset",
                vec![Expr::dream_p(o.clone()), Expr::i(0), Expr::i(size as i64)],
            );
            if let Some(name) = &ctor_name {
                let mut call_args = vec![o.clone()];
                call_args.extend(arg_es.iter().cloned());
                b.call(name.clone(), call_args);
            }
            o
        })
    }

    /// Construct a value struct directly into `dest` (a shadow-stack slot): zero
    /// the slot, run the constructor in place. Skips the heap-temp + memcpy +
    /// free round-trip of [`Self::emit_new`] for value-local assignments.
    pub(super) fn struct_new_at(
        &mut self,
        dest: Expr,
        ty: dream_types::TypeId,
        _def: dream_types::DefId,
        ctor: Option<dream_types::DefId>,
        args: &[crate::Operand],
    ) {
        let layout = self.cx.nstruct(ty).unwrap_or_else(|| {
            crate::internal_error!("missing layout for struct allocation {ty:?}")
        });
        for a in args {
            self.retain_rc_global_sink(true, a);
        }
        let arg_es: Vec<Expr> = args.iter().map(|a| self.operand(a)).collect();
        let ctor_name = ctor.map(|c| runtime_c_name(&self.cx.callee_c(c, &[])));
        self.b
            .call("memset", vec![Expr::dream_p(dest.clone()), Expr::i(0), Expr::i(layout.size as i64)]);
        if let Some(name) = ctor_name {
            let mut call_args = vec![dest];
            call_args.extend(arg_es);
            self.b.call(name, call_args);
        }
    }

    fn concat_parts(&mut self, parts: &[crate::Operand]) -> Expr {
        if parts.is_empty() {
            return Expr::id(self.cx.str_sym(""));
        }
        if parts.len() == 1 {
            return self.operand(&parts[0]);
        }
        let elems: Vec<Expr> = parts.iter().map(|p| self.operand(p)).collect();
        let n = elems.len();
        self.b.temp(
            CTy::Ptr,
            Some(Expr::Gnu {
                stmts: vec![Stmt::Decl {
                    align: None,
                    static_: false,
                    const_: false,
                    ty: CTy::Array {
                        elem: Box::new(CTy::Ptr),
                        len: n,
                    },
                    name: "__parts".into(),
                    init: Some(Expr::Compound(elems)),
                }],
                result: Box::new(Expr::call(
                    "dream_concat_n",
                    vec![Expr::id("__parts"), Expr::i(n as i64)],
                )),
            }),
        )
    }

    pub(super) fn union_new_at(
        &mut self,
        dest: Expr,
        ty: dream_types::TypeId,
        variant: usize,
        args: &[crate::Operand],
    ) {
        let u = self
            .cx
            .nunion(ty)
            .unwrap_or_else(|| crate::internal_error!("missing union layout {ty:?}"));
        let var = u
            .variants
            .iter()
            .find(|v| v.discriminant as usize == variant)
            .unwrap_or_else(|| crate::internal_error!("missing variant {variant}"));
        let size = u.size;
        self.b.call(
            "memset",
            vec![
                Expr::dream_p(dest.clone()),
                Expr::i(0),
                Expr::i(size as i64),
            ],
        );
        self.b.stmt(Stmt::store(
            CTy::I32,
            Expr::dream_p(dest.clone()),
            Expr::i(variant as i64),
        ));
        for (i, arg) in args.iter().enumerate() {
            let fld = &var.fields[i];
            if self.cx.interner.is_value_type(fld.ty) {
                let fsz = elem_size(self.cx, fld.ty);
                let src = self.operand(arg);
                self.b.call(
                    "memcpy",
                    vec![
                        Expr::ptr_add(dest.clone(), Expr::i(fld.offset as i64)),
                        Expr::dream_p(src),
                        Expr::i(fsz as i64),
                    ],
                );
                if !matches!(arg, crate::Operand::Copy(crate::Place::Local(_))) {
                    let at = Expr::cast(
                        CTy::Ptr,
                        Expr::ptr_add(dest.clone(), Expr::i(fld.offset as i64)),
                    );
                    self.value_refs(fld.ty, at, true);
                }
                continue;
            }
            let cast = load_cast(self.cx, fld.ty);
            let val = self.operand(arg);
            self.b.stmt(Stmt::store(
                cast.clone(),
                Expr::ptr_add(dest.clone(), Expr::i(fld.offset as i64)),
                val,
            ));
            if self.cx.interner.is_rc_tracked(fld.ty)
                && crate::backend::shared::unique_container_move_local(
                    self.f,
                    self.cx.interner,
                    arg,
                )
                .is_none()
            {
                self.b.call(
                    crate::backend::c::release::retain_sym(self.cx, fld.ty),
                    vec![Expr::load(
                        cast,
                        Expr::ptr_add(dest.clone(), Expr::i(fld.offset as i64)),
                    )],
                );
            }
        }
    }

    fn emit_union_new(
        &mut self,
        def: dream_types::DefId,
        ty: dream_types::TypeId,
        variant: usize,
        args: &[crate::Operand],
    ) -> Expr {
        let size = self
            .cx
            .nunion(ty)
            .unwrap_or_else(|| crate::internal_error!("missing union layout {ty:?}"))
            .size;
        let tag = self.cx.type_tag(ty, def);
        let arg_ops: Vec<_> = args.to_vec();
        let cx = self.cx;
        let func = self.f;
        self.b.expr_block(|b| {
            let o = b.temp(
                CTy::Ptr,
                Some(Expr::call(
                    "dream_malloc",
                    vec![Expr::i(size as i64), Expr::i(tag as i64)],
                )),
            );
            let mut e = Emitter::new(cx, func, b);
            e.union_new_at(o.clone(), ty, variant, &arg_ops);
            o
        })
    }

    fn emit_array_lit(&mut self, elem_ty: dream_types::TypeId, elems: &[crate::Operand]) -> Expr {
        let es = elem_size(self.cx, elem_ty);
        let n = elems.len();
        let size = 4 + es * n as u32;
        let cast = load_cast(self.cx, elem_ty);
        let values: Vec<Expr> = elems.iter().map(|e| self.operand(e)).collect();
        let skip_retain: Vec<bool> = elems
            .iter()
            .map(|e| {
                crate::backend::shared::unique_container_move_local(self.f, self.cx.interner, e)
                    .is_some()
            })
            .collect();
        let is_val = self.cx.interner.is_value_type(elem_ty);
        let rc = self.cx.interner.is_rc_tracked(elem_ty);
        let cx = self.cx;
        let func = self.f;
        self.b.expr_block(move |b| {
            let o = b.temp(
                CTy::Ptr,
                Some(Expr::call(
                    "dream_malloc",
                    vec![Expr::i(size as i64), Expr::i(crate::abi::TAG_ARRAY as i64)],
                )),
            );
            b.call(
                "memset",
                vec![Expr::dream_p(o.clone()), Expr::i(0), Expr::i(size as i64)],
            );
            b.stmt(Stmt::store(
                CTy::I32,
                Expr::dream_p(o.clone()),
                Expr::i(n as i64),
            ));
            for (i, value) in values.iter().enumerate() {
                let at = Expr::add(
                    Expr::ptr_add(o.clone(), super::types::len_prefix()),
                    Expr::mul(Expr::i(i as i64), Expr::i(es as i64)),
                );
                if is_val {
                    b.call(
                        "memcpy",
                        vec![at.clone(), Expr::dream_p(value.clone()), Expr::i(es as i64)],
                    );
                    let mut e = Emitter::new(cx, func, b);
                    e.value_refs(elem_ty, Expr::cast(CTy::Ptr, at), true);
                } else {
                    b.stmt(Stmt::store(cast.clone(), at.clone(), value.clone()));
                    if rc && !skip_retain[i] {
                        b.call(
                            crate::backend::c::release::retain_sym(cx, elem_ty),
                            vec![Expr::load(cast.clone(), at)],
                        );
                    }
                }
            }
            o
        })
    }

    fn emit_tuple(&mut self, ty: dream_types::TypeId, elems: &[crate::Operand]) -> Expr {
        let layout = self
            .cx
            .nstruct(ty)
            .unwrap_or_else(|| crate::internal_error!("missing tuple layout {ty:?}"));
        let size = layout.size.max(1);
        let tag = self.cx.type_tag(ty, dream_types::DefId(0));
        let fields: Vec<_> = layout.fields.clone();
        let values: Vec<Expr> = elems.iter().map(|e| self.operand(e)).collect();
        let skip_retain: Vec<bool> = elems
            .iter()
            .map(|e| {
                crate::backend::shared::unique_container_move_local(self.f, self.cx.interner, e)
                    .is_some()
            })
            .collect();
        let is_val: Vec<bool> = fields
            .iter()
            .map(|f| self.cx.interner.is_value_type(f.ty))
            .collect();
        let is_rc: Vec<bool> = fields
            .iter()
            .map(|f| self.cx.interner.is_rc_tracked(f.ty))
            .collect();
        let casts: Vec<CTy> = fields.iter().map(|f| load_cast(self.cx, f.ty)).collect();
        let sizes: Vec<u32> = fields.iter().map(|f| elem_size(self.cx, f.ty)).collect();
        let cx = self.cx;
        self.b.expr_block(move |b| {
            let o = b.temp(
                CTy::Ptr,
                Some(Expr::call(
                    "dream_malloc",
                    vec![Expr::i(size as i64), Expr::i(tag as i64)],
                )),
            );
            b.call(
                "memset",
                vec![Expr::dream_p(o.clone()), Expr::i(0), Expr::i(size as i64)],
            );
            for (i, value) in values.iter().enumerate() {
                if let Some(fld) = fields.get(i) {
                    if is_val[i] {
                        b.call(
                            "memcpy",
                            vec![
                                Expr::ptr_add(o.clone(), Expr::i(fld.offset as i64)),
                                Expr::dream_p(value.clone()),
                                Expr::i(sizes[i] as i64),
                            ],
                        );
                    } else {
                        b.stmt(Stmt::store(
                            casts[i].clone(),
                            Expr::ptr_add(o.clone(), Expr::i(fld.offset as i64)),
                            value.clone(),
                        ));
                        if is_rc[i] && !skip_retain[i] {
                            b.call(
                                crate::backend::c::release::retain_sym(cx, fld.ty),
                                vec![Expr::load(
                                    casts[i].clone(),
                                    Expr::ptr_add(o.clone(), Expr::i(fld.offset as i64)),
                                )],
                            );
                        }
                    }
                }
            }
            o
        })
    }

    fn emit_cast(
        &mut self,
        v: &crate::Operand,
        from: dream_types::TypeId,
        to: dream_types::TypeId,
    ) -> Expr {
        let src = self.operand(v);
        if from == to {
            return src;
        }
        let fk = self.cx.interner.kind(from);
        let tk = self.cx.interner.kind(to);
        if self.cx.target.is_wasm32() {
            if let Some(sym) = super::js_marshal::cast_c_sym(self.cx, from, to) {
                if matches!(self.cx.interner.kind(from), TyKind::Js)
                    && self.cx.interner.is_value_type(to)
                {
                    crate::internal_error!(
                        "js→value-struct cast must be stored in place (got stack emit of {sym})"
                    );
                }
                return Expr::call(sym, vec![src]);
            }
            if matches!(tk, TyKind::Js) {
                if let TyKind::Prim(p) = fk {
                    let (method, promote) = match p {
                        PrimTy::Int | PrimTy::UInt | PrimTy::Byte | PrimTy::Char => ("box_int", false),
                        PrimTy::Long | PrimTy::ULong => ("box_long", false),
                        PrimTy::Float => ("box_double", true),
                        PrimTy::Double => ("box_double", false),
                        PrimTy::Bool => ("box_bool", false),
                        PrimTy::String => ("box_string", false),
                    };
                    let arg = if promote {
                        Expr::cast(CTy::F64, src)
                    } else {
                        src
                    };
                    return Expr::call(super::js_marshal::js_bridge(self.cx, method), vec![arg]);
                }
            }
            if matches!(fk, TyKind::Js) {
                if let TyKind::Prim(p) = tk {
                    let (method, demote) = match p {
                        PrimTy::Int | PrimTy::UInt | PrimTy::Byte | PrimTy::Char => ("as_int", false),
                        PrimTy::Long | PrimTy::ULong => ("as_long", false),
                        PrimTy::Float => ("as_double", true),
                        PrimTy::Double => ("as_double", false),
                        PrimTy::Bool => ("as_bool", false),
                        PrimTy::String => ("as_string", false),
                    };
                    let v = Expr::call(super::js_marshal::js_bridge(self.cx, method), vec![src]);
                    return if demote {
                        Expr::cast(CTy::F32, v)
                    } else {
                        v
                    };
                }
            }
        }
        let to_is_ref_box = matches!(tk, TyKind::Object | TyKind::Interface(..));
        if to_is_ref_box && self.cx.interner.is_value_type(from) {
            let size = elem_size(self.cx, from);
            let tag = self.cx.type_tag(from, dream_types::DefId(0));
            return self.b.expr_block(move |b| {
                let box_ = b.temp(
                    CTy::Ptr,
                    Some(Expr::call(
                        "dream_malloc",
                        vec![Expr::i(size as i64), Expr::i(tag as i64)],
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
                box_
            });
        }
        match (fk, tk) {
            (TyKind::Prim(PrimTy::Int), TyKind::Prim(PrimTy::Long)) => {
                Expr::cast(CTy::I64, Expr::cast(CTy::I32, src))
            }
            (TyKind::Prim(PrimTy::Int), TyKind::Prim(PrimTy::Double)) => {
                Expr::cast(CTy::F64, Expr::cast(CTy::I32, src))
            }
            (TyKind::Prim(PrimTy::Int), TyKind::Prim(PrimTy::Float)) => {
                Expr::cast(CTy::F32, Expr::cast(CTy::I32, src))
            }
            // Unsigned sources must zero-extend (via U32/U64) before conversion; a direct
            // I32→F64 cast would sign-extend values ≥ 2^31.
            (TyKind::Prim(PrimTy::UInt), TyKind::Prim(PrimTy::Double)) => {
                Expr::cast(CTy::F64, Expr::cast(CTy::U32, src))
            }
            (TyKind::Prim(PrimTy::UInt), TyKind::Prim(PrimTy::Float)) => {
                Expr::cast(CTy::F32, Expr::cast(CTy::U32, src))
            }
            (TyKind::Prim(PrimTy::UInt), TyKind::Prim(PrimTy::Long)) => {
                Expr::cast(CTy::I64, Expr::cast(CTy::U32, src))
            }
            // `byte`/`char` are I32-backed; their numeric value is already sign-safe.
            (TyKind::Prim(PrimTy::Byte | PrimTy::Char), TyKind::Prim(PrimTy::Double)) => {
                Expr::cast(CTy::F64, Expr::cast(CTy::I32, src))
            }
            (TyKind::Prim(PrimTy::Byte | PrimTy::Char), TyKind::Prim(PrimTy::Float)) => {
                Expr::cast(CTy::F32, Expr::cast(CTy::I32, src))
            }
            (TyKind::Prim(PrimTy::ULong), TyKind::Prim(PrimTy::Double)) => {
                Expr::cast(CTy::F64, Expr::cast(CTy::Named("unsigned long long"), src))
            }
            (TyKind::Prim(PrimTy::ULong), TyKind::Prim(PrimTy::Float)) => {
                Expr::cast(CTy::F32, Expr::cast(CTy::Named("unsigned long long"), src))
            }
            (TyKind::Prim(PrimTy::Float), TyKind::Prim(PrimTy::Double)) => {
                Expr::cast(CTy::F64, Expr::cast(CTy::F32, src))
            }
            (TyKind::Prim(PrimTy::Double), TyKind::Prim(PrimTy::Float)) => {
                Expr::cast(CTy::F32, src)
            }
            (TyKind::Prim(PrimTy::Double), TyKind::Prim(PrimTy::Int)) => Expr::cast(CTy::I32, src),
            (TyKind::Prim(PrimTy::Float), TyKind::Prim(PrimTy::Int)) => Expr::cast(CTy::I32, src),
            (TyKind::Prim(PrimTy::Long), TyKind::Prim(PrimTy::Int)) => Expr::cast(CTy::I32, src),
            (_, TyKind::Object) => match fk {
                TyKind::Prim(PrimTy::Int) => {
                    Expr::call("dream_box_int", vec![Expr::cast(CTy::I32, src)])
                }
                TyKind::Prim(PrimTy::Float) => {
                    Expr::call("dream_box_float", vec![Expr::cast(CTy::F32, src)])
                }
                TyKind::Prim(PrimTy::Double) => {
                    Expr::call("dream_box_double", vec![Expr::cast(CTy::F64, src)])
                }
                TyKind::Prim(PrimTy::Bool) => {
                    Expr::call("dream_box_bool", vec![Expr::cast(CTy::I32, src)])
                }
                TyKind::Prim(PrimTy::Char) => {
                    Expr::call("dream_box_char", vec![Expr::cast(CTy::I32, src)])
                }
                TyKind::Prim(PrimTy::Long) => {
                    Expr::call("dream_box_long", vec![Expr::cast(CTy::I64, src)])
                }
                TyKind::Prim(PrimTy::UInt) => {
                    Expr::call("dream_box_uint", vec![Expr::cast(CTy::I32, src)])
                }
                TyKind::Prim(PrimTy::ULong) => {
                    Expr::call("dream_box_ulong", vec![Expr::cast(CTy::I64, src)])
                }
                TyKind::Prim(PrimTy::Byte) => {
                    Expr::call("dream_box_byte", vec![Expr::cast(CTy::I32, src)])
                }
                _ => src,
            },
            (TyKind::Object, TyKind::Prim(p)) => match p {
                PrimTy::Int => self.checked_unbox(src, to, "dream_unbox_int"),
                PrimTy::Float => self.checked_unbox(src, to, "dream_unbox_float"),
                PrimTy::Double => self.checked_unbox(src, to, "dream_unbox_double"),
                PrimTy::Bool => self.checked_unbox(src, to, "dream_unbox_bool"),
                PrimTy::Char => self.checked_unbox(src, to, "dream_unbox_char"),
                PrimTy::Long => self.checked_unbox(src, to, "dream_unbox_long"),
                PrimTy::UInt => self.checked_unbox(src, to, "dream_unbox_uint"),
                PrimTy::ULong => self.checked_unbox(src, to, "dream_unbox_ulong"),
                PrimTy::Byte => self.checked_unbox(src, to, "dream_unbox_byte"),
                PrimTy::String => src,
            },
            _ => src,
        }
    }

    fn checked_unbox(&mut self, src: Expr, ty: dream_types::TypeId, unbox: &'static str) -> Expr {
        let tag = runtime_tag(self.cx, ty);
        let panic = Expr::id(
            self.cx
                .str_sym(crate::backend::shared::panic_msgs::INVALID_CAST),
        );
        self.b.expr_block(move |b| {
            let boxed = b.temp(CTy::Ptr, Some(Expr::cast(CTy::Ptr, src.clone())));
            b.stmt(Stmt::if_(
                Expr::ne(
                    Expr::call("dream_object_tag", vec![boxed.clone()]),
                    Expr::i(tag as i64),
                ),
                Stmt::call("dream_panic", vec![panic.clone()]),
            ));
            Expr::call(unbox, vec![boxed])
        })
    }

    pub(super) fn hash_code_expr(&self, ty: dream_types::TypeId, value: Expr) -> Expr {
        hash_code_of(self.cx, ty, value)
    }

    pub(super) fn to_string_fn(&self, ty: dream_types::TypeId) -> String {
        to_string_fn(self.cx, ty)
    }
}

pub(super) fn hash_code_of(cx: &super::ctx::Cx<'_>, ty: dream_types::TypeId, value: Expr) -> Expr {
    match cx.interner.kind(ty) {
        TyKind::Prim(PrimTy::String) => Expr::call("dream_string_hash", vec![value]),
        TyKind::Prim(PrimTy::Float) => Expr::call("dream_bitcast_f32", vec![value]),
        TyKind::Prim(PrimTy::Double) => Expr::call("dream_hash_double", vec![value]),
        TyKind::Prim(PrimTy::Long | PrimTy::ULong) => Expr::call("dream_hash_long", vec![value]),
        TyKind::Prim(_) | TyKind::Enum(_) => Expr::cast(CTy::I32, value),
        _ => {
            if let Some(l) = cx.nstruct(ty) {
                Expr::call(c_ident(&format!("{}_hash_code", l.name)), vec![value])
            } else if let Some(u) = cx.nunion(ty) {
                Expr::call(c_ident(&format!("{}_hash_code", u.name)), vec![value])
            } else {
                Expr::call("dream_object_hash_code", vec![value])
            }
        }
    }
}

pub(super) fn to_string_fn(cx: &super::ctx::Cx<'_>, ty: dream_types::TypeId) -> String {
    match cx.interner.kind(ty) {
        TyKind::Prim(PrimTy::Int) => "dream_int_to_string_fast".into(),
        TyKind::Prim(PrimTy::UInt) => "dream_uint_to_string".into(),
        TyKind::Prim(PrimTy::Long) => "dream_long_to_string".into(),
        TyKind::Prim(PrimTy::ULong) => "dream_ulong_to_string".into(),
        TyKind::Prim(PrimTy::Byte) => "dream_byte_to_string".into(),
        TyKind::Prim(PrimTy::Bool) => "dream_bool_to_string".into(),
        TyKind::Prim(PrimTy::Char) => "dream_char_to_string".into(),
        TyKind::Prim(PrimTy::Float) => "dream_float_to_string".into(),
        TyKind::Prim(PrimTy::Double) => "dream_double_to_string".into(),
        TyKind::Prim(PrimTy::String) => String::new(),
        TyKind::Enum(_) => "dream_int_to_string".into(),
        TyKind::Array(e) => c_ident(&format!("array_to_string_t{}", e.0)),
        _ => {
            if let Some(l) = cx.nstruct(ty) {
                c_ident(&format!("{}_to_string", l.name))
            } else if let Some(u) = cx.nunion(ty) {
                c_ident(&format!("{}_to_string", u.name))
            } else {
                "dream_object_to_string".into()
            }
        }
    }
}

fn runtime_tag(cx: &super::ctx::Cx<'_>, ty: dream_types::TypeId) -> i32 {
    match cx.interner.kind(ty) {
        TyKind::Prim(PrimTy::Int) => crate::abi::TAG_INT,
        TyKind::Prim(PrimTy::Float) => crate::abi::TAG_FLOAT,
        TyKind::Prim(PrimTy::Double) => crate::abi::TAG_DOUBLE,
        TyKind::Prim(PrimTy::Bool) => crate::abi::TAG_BOOL,
        TyKind::Prim(PrimTy::String) => crate::abi::TAG_STRING,
        TyKind::Prim(PrimTy::Char) => crate::abi::TAG_CHAR,
        TyKind::Prim(PrimTy::Long) => crate::abi::TAG_LONG,
        TyKind::Prim(PrimTy::UInt) => crate::abi::TAG_UINT,
        TyKind::Prim(PrimTy::ULong) => crate::abi::TAG_ULONG,
        TyKind::Prim(PrimTy::Byte) => crate::abi::TAG_BYTE,
        TyKind::Array(_) => crate::abi::TAG_ARRAY,
        _ => cx.type_tag(ty, dream_types::DefId(0)),
    }
}

//! Rvalues: arithmetic dispatch, strings, allocation and construction, unions and protocol hooks.

use super::fx::{Fx, V};
use super::ir::{Ty, Value};
use crate::backend::shared::abi_types::{elem_size, mem_ty, runtime_c_name};
use crate::backend::shared::glue::{release_sym, retain_sym};
use crate::backend::shared::protocol_names::{HashFn, hash_fn, runtime_tag, to_string_fn};
use crate::{Operand, Rvalue, UnOp};
use dream_types::{DefId, PrimTy, TyKind, TypeId};

/// Values nested in a union or array payload are only 4-aligned.
const PAYLOAD_ALIGN: u32 = 4;

impl<'l, 'a> Fx<'l, 'a> {
    /// `dest` is the destination's type when known; integer arithmetic is performed at it.
    pub fn rvalue(&mut self, rv: &Rvalue, dest: Option<TypeId>) -> V {
        match rv {
            Rvalue::Use(o) => self.operand(o),
            Rvalue::Move { src, cast } => {
                let o = Operand::Copy(crate::Place::Local(*src));
                match cast {
                    Some((from, to)) => self.emit_cast(&o, *from, *to),
                    None => self.operand(&o),
                }
            }
            Rvalue::Select {
                cond,
                then_val,
                else_val,
            } => {
                let c = self.operand(cond);
                let a = self.operand(then_val);
                let b = self.operand(else_val);
                let c = self.truthy(&c);
                self.select_v(&c, &a, &b)
            }
            Rvalue::Binary(op, a, b) => {
                let is_string = |o: &Operand| match o {
                    Operand::Copy(crate::Place::Local(l)) => {
                        matches!(
                            self.interner.kind(self.f.local_ty(*l)),
                            TyKind::Prim(PrimTy::String)
                        )
                    }
                    Operand::Const(crate::Const::Str(_)) => true,
                    _ => false,
                };
                if matches!(*op, crate::BinOp::Eq | crate::BinOp::Ne) && is_string(a) {
                    let (x, y) = (self.operand(a), self.operand(b));
                    let eq = self.call_v("dream_string_eq", &[x, y]);
                    if *op == crate::BinOp::Eq {
                        return eq;
                    }
                    return self.not_v(&eq);
                }
                match self.binary_int_ty(*op, a, b, dest) {
                    Some(ty) => self.int_binary(*op, ty, a, b),
                    None => {
                        let (x, y) = (self.operand(a), self.operand(b));
                        self.c_binary(*op, &x, &y)
                    }
                }
            }
            Rvalue::CheckedBinary(op, a, b) => match self.binary_int_ty(*op, a, b, dest) {
                Some(ty) => self.checked_binary(*op, ty, a, b),
                None => crate::internal_error!("checked {op:?} on a non-integer operand"),
            },
            Rvalue::CheckedNeg(a) => match self.unary_int_ty(a, dest) {
                Some(ty) => self.checked_neg(ty, a),
                None => crate::internal_error!("checked negation of a non-integer operand"),
            },
            Rvalue::Unary(UnOp::Not, a) => {
                let x = self.operand(a);
                self.not_v(&x)
            }
            Rvalue::Unary(op, a) => match self.unary_int_ty(a, dest) {
                Some(ty) => self.int_unary(*op, ty, a),
                None => {
                    let x = self.operand(a);
                    self.c_unary(*op, &x)
                }
            },
            Rvalue::StrLen(s) => {
                let s = self.operand(s);
                self.call_v("dream_str_len", &[s])
            }
            Rvalue::StrByteSize(s) => {
                let s = self.operand(s);
                self.call_v("dream_str_byte_size", &[s])
            }
            Rvalue::CharAt(s, i, _) => {
                let (s, i) = (self.operand(s), self.operand(i));
                let i = self.conv_v(&i, &Ty::I32, false);
                let c = self.call_v("dream_char_at_u", &[s, i]);
                self.conv_v(&V::u(c.v), &Ty::I32, false)
            }
            Rvalue::ByteAt(s, i, _) => {
                let (s, i) = (self.operand(s), self.operand(i));
                let i = self.conv_v(&i, &Ty::I32, false);
                let c = self.call_v("dream_byte_at_u", &[s, i]);
                self.conv_v(&V::u(c.v), &Ty::I32, false)
            }
            Rvalue::StrBytes(s) => {
                let s = self.operand(s);
                let p = self.call_v("dream_str_bytes", &[s]);
                self.as_ref(&V::u(p.v))
            }
            Rvalue::LoadU8(p, i) => self.load_unit(p, i, Ty::I8, 1),
            Rvalue::LoadU16(p, i) => self.load_unit(p, i, Ty::I16, 2),
            Rvalue::ArrayNew { elem_ty, len, .. } => {
                let es = elem_size(&self.l.cx, *elem_ty);
                let n = self.operand(len);
                let array = self.call_v("dream_array_new", &[n, V::i32(es as i64)]);
                self.install_array_info(&array, *elem_ty);
                array
            }
            Rvalue::HashCode(o) => {
                let ty = self.operand_ty(o);
                let v = self.operand(o);
                self.hash_code_of(ty, &v)
            }
            Rvalue::ToString(o) => {
                let ty = self.operand_ty(o);
                let conv = to_string_fn(&self.l.cx, ty);
                let v = self.operand(o);
                if conv.is_empty() {
                    v
                } else {
                    self.call_v(&conv, &[v])
                }
            }
            Rvalue::Concat(parts) => self.concat_parts(parts),
            Rvalue::ConcatInt {
                prefix,
                value,
                suffix,
            } => {
                let p = self.operand(prefix);
                let v = self.operand(value);
                let v = self.conv_v(&v, &Ty::I32, false);
                let s = self.operand(suffix);
                self.call_v("dream_concat_str_int_str", &[p, v, s])
            }
            Rvalue::EnumName { value, arms } => {
                let v = self.operand(value);
                let v64 = self.conv(&v, &Ty::I64);
                let mut e = self.str_v("");
                for (k, name) in arms.iter().rev() {
                    let hit = self.w.icmp("eq", &v64, &Value::i64(*k));
                    let s = self.str_v(name);
                    e = V::u(self.w.select(&hit, &s.v, &e.v));
                }
                e
            }
            Rvalue::Call { callee, args } => self
                .call_expr(callee, args)
                .unwrap_or_else(|| crate::internal_error!("void call used as a value")),
            Rvalue::IndirectCall { target, args, sig } => {
                let r = self.indirect_expr(target, args, *sig).unwrap_or_else(|| {
                    crate::internal_error!("void indirect call used as a value")
                });
                match self.interner.kind(*sig) {
                    TyKind::Func(_, ret)
                        if matches!(
                            self.interner.kind(*ret),
                            TyKind::Prim(
                                PrimTy::Int
                                    | PrimTy::UInt
                                    | PrimTy::Bool
                                    | PrimTy::Byte
                                    | PrimTy::Char
                            ) | TyKind::Enum(_)
                        ) =>
                    {
                        let wide = self.conv_v(&r, &Ty::I64, false);
                        self.conv_v(&wide, &Ty::I32, false)
                    }
                    _ => r,
                }
            }
            Rvalue::InterfaceCall {
                receiver,
                iface_id,
                method_slot,
                sig,
                args,
                ..
            } => self
                .iface_expr(receiver, *iface_id, *method_slot, *sig, args)
                .unwrap_or_else(|| crate::internal_error!("void interface call used as a value")),
            Rvalue::FuncRef(callee) => {
                let idx = self
                    .l
                    .cx
                    .ft
                    .get(&(callee.def, callee.args.clone()))
                    .copied()
                    .unwrap_or(0);
                V::i32(idx as i64)
            }
            Rvalue::New {
                ty,
                ctor,
                args,
                policy,
                ..
            } => self.emit_new_in(*ty, ctor.as_ref().map(|c| c.def), args, None, *policy, ctor.as_ref().is_some_and(|c| c.batched)),
            Rvalue::Tuple { ty, elems } => self.emit_tuple(*ty, elems),
            Rvalue::UnionNew {
                ty, variant, args, ..
            } => self.emit_union_new(*ty, *variant, args),
            Rvalue::ArrayLit { elem_ty, elems } => self.emit_array_lit(*elem_ty, elems),
            Rvalue::ArrayLen(a) => {
                let a = self.operand(a);
                let p = self.ptr(&a);
                self.load_ty(Ty::I32, &p, 4, false)
            }
            Rvalue::ToBytes { value, ty } => {
                let sz = elem_size(&self.l.cx, *ty) as i64;
                let v = self.operand(value);
                match self.interner.kind(*ty) {
                    TyKind::Prim(PrimTy::String) => self.call_v("dream_to_bytes", &[v, V::i32(sz)]),
                    TyKind::Prim(_) | TyKind::Enum(_) => {
                        let (t, _) =
                            super::fx::mem_ll(mem_ty(&self.l.cx, *ty), &self.h(), &self.word());
                        let tmp = self.w.alloca(t.clone(), 8);
                        self.store_ty(&t, &tmp, &v, 8);
                        let addr = self.as_ref(&V::s(tmp));
                        self.call_v("dream_to_bytes", &[addr, V::i32(sz)])
                    }
                    _ => self.call_v("dream_to_bytes", &[v, V::i32(sz)]),
                }
            }
            Rvalue::FromBytes { bytes, ty } => {
                let tag = self.l.cx.type_tag(*ty);
                let sz = elem_size(&self.l.cx, *ty) as i64;
                let b = self.operand(bytes);
                let boxed = self.call_v("dream_from_bytes", &[b, V::i64(sz), V::i32(tag as i64)]);
                match self.interner.kind(*ty) {
                    TyKind::Prim(PrimTy::String) => boxed,
                    TyKind::Prim(_) | TyKind::Enum(_) => {
                        let p = self.ptr(&boxed);
                        let m = mem_ty(&self.l.cx, *ty);
                        let v = self.load_mem(m, &p, 8);
                        self.call("dream_release", &[boxed]);
                        v
                    }
                    _ => boxed,
                }
            }
            Rvalue::ArrayRealloc {
                elem_ty,
                array,
                new_len,
            } => {
                let es = elem_size(&self.l.cx, *elem_ty) as i64;
                let arr = self.operand(array);
                let len = self.operand(new_len);
                // RC-tracked elements must release dropped tail slots on shrink.
                if self.is_rc(*elem_ty) {
                    let rel = release_sym(&self.l.cx, *elem_ty);
                    let f = V::s(self.l.fn_ref(&rel));
                    return self.call_v("dream_array_realloc_rc", &[arr, len, V::i32(es), f]);
                }
                self.call_v("dream_array_realloc", &[arr, len, V::i32(es)])
            }
            Rvalue::Cast(v, from, to) => self.emit_cast(v, *from, *to),
            Rvalue::Discriminant { base, ty } => {
                let b = self.operand(base);
                self.union_discriminant(*ty, &b)
            }
            Rvalue::UnionField {
                base,
                ty,
                variant,
                field,
            } => {
                let b = self.operand(base);
                self.union_field(*ty, *variant, *field, &b)
            }
            Rvalue::TypeName(o) => {
                let v = self.operand(o);
                self.call_v("dream_object_type_name", &[v])
            }
            Rvalue::IsType(o, ty) => {
                let tag = runtime_tag(&self.l.cx, *ty);
                let v = self.operand(o);
                let t = self.call_v("dream_object_tag", &[v]);
                let t = self.conv(&t, &Ty::I32);
                let c = self.w.icmp("eq", &t, &Value::i32(tag as i64));
                self.bool_v(&c)
            }
            Rvalue::JsCall {
                callee,
                target,
                via,
                method,
                args,
            } => self
                .js_call(callee, target, via, method, args)
                .unwrap_or_else(|| crate::internal_error!("JS call used as a value returns void")),
        }
    }

    pub fn js_call_native(
        &mut self,
        target: &Operand,
        via: &Option<Operand>,
        method: &Option<Operand>,
        argc: usize,
    ) -> V {
        let t = self.operand(target);
        let v = match via {
            Some(o) => self.operand(o),
            None => V::i32(0),
        };
        let m = match method {
            Some(o) => self.operand(o),
            None => V::s(Value::zero(self.h())),
        };
        self.call_v("dream_js_call", &[t, v, m, V::i32(argc as i64)])
    }

    fn select_v(&mut self, c: &Value, a: &V, b: &V) -> V {
        if a.ty() == b.ty() {
            return V {
                v: self.w.select(c, &a.v, &b.v),
                unsigned: a.unsigned,
            };
        }
        let r = self.c_binary(crate::BinOp::Add, a, b);
        let t = r.ty().clone();
        let (x, y) = (self.conv(a, &t), self.conv(b, &t));
        V {
            v: self.w.select(c, &x, &y),
            unsigned: r.unsigned,
        }
    }

    pub fn not_v(&mut self, x: &V) -> V {
        let t = self.truthy(x);
        let n = self.w.bin("xor", &t, &Value::i1(true));
        self.bool_v(&n)
    }

    fn c_unary(&mut self, op: UnOp, x: &V) -> V {
        if x.ty().is_float() {
            return V::s(
                self.w
                    .assign(x.ty().clone(), format!("fneg {}", x.v.typed())),
            );
        }
        let t = if x.ty().int_bits().unwrap_or(64) < 32 {
            Ty::I32
        } else {
            x.ty().clone()
        };
        let v = self.conv(x, &t);
        let r = match op {
            UnOp::Neg => self.w.bin("sub", &Value::zero(t), &v),
            _ => self.w.bin("xor", &v, &Value::int(t, -1)),
        };
        V {
            v: r,
            unsigned: x.unsigned,
        }
    }

    fn load_unit(&mut self, p: &Operand, i: &Operand, ty: Ty, size: i64) -> V {
        let base = self.operand(p);
        let idx = self.operand(i);
        let idx = self.conv_v(&idx, &Ty::I32, false);
        let idx = self.conv(&idx, &Ty::I64);
        let off = self.w.bin("mul", &idx, &Value::i64(size));
        let bp = self.ptr(&base);
        let at = self.w.gep_i8(&bp, &off);
        let u = self.load_ty(ty, &at, size as u32, true);
        self.conv_v(&u, &Ty::I32, false)
    }

    pub fn hash_code_of(&mut self, ty: TypeId, v: &V) -> V {
        match hash_fn(&self.l.cx, ty) {
            HashFn::Identity => self.conv_v(v, &Ty::I32, false),
            HashFn::Call(f) => self.call_v(&f, std::slice::from_ref(v)),
        }
    }

    fn concat_parts(&mut self, parts: &[Operand]) -> V {
        if parts.is_empty() {
            return self.str_v("");
        }
        if parts.len() == 1 {
            return self.operand(&parts[0]);
        }
        let vals: Vec<V> = parts.iter().map(|p| self.operand(p)).collect();
        let n = vals.len();
        let h = self.h();
        let ps = self.l.cx.target.abi().ptr_size as i64;
        let buf = self.w.alloca(Ty::Array(n as u64, Box::new(h.clone())), 8);
        for (i, v) in vals.iter().enumerate() {
            let at = self.w.gep_const(&buf, i as i64 * ps);
            self.store_ty(&h, &at, v, 8);
        }
        self.call_v("dream_concat_n", &[V::s(buf), V::i32(n as i64)])
    }

    fn ctor_name(&self, ctor: Option<DefId>) -> Option<String> {
        ctor.map(|c| runtime_c_name(&self.l.cx.callee_sym(c, &[])))
    }

    /// `New` on the heap, or in the frame buffer `frame` (an immortal, never-freed block).
    pub fn emit_new_in(
        &mut self,
        ty: TypeId,
        ctor: Option<DefId>,
        args: &[Operand],
        frame: Option<Value>,
        policy: crate::AllocPolicy,
        batched: bool,
    ) -> V {
        let layout = self.l.cx.nstruct(ty).unwrap_or_else(|| {
            crate::internal_error!("missing layout for struct allocation {ty:?}")
        });
        let shared = self.interner.is_shared_type(ty);
        let mut size = layout.size;
        if shared {
            size += crate::abi::HEADER_LOCK_WORD_SIZE;
        }
        let tag = self.l.cx.type_tag(ty);
        for a in args {
            self.retain_rc_global_sink(true, a);
        }
        let vals: Vec<V> = args.iter().map(|a| self.operand(a)).collect();
        let ctor_name = self.ctor_name(ctor);
        let borrows_builder_gate = self.construction_gate.is_some();
        let gate = if batched && frame.is_none() && !shared {
            if let Some(outer) = self.construction_gate.clone() {
                // A verified builder already owns the boundary; zero preserves its private
                // region path and two borrows the gate without prematurely ending that boundary.
                let private = self.w.icmp("eq", &outer.v, &Value::i32(0));
                Some(V::s(self.w.select(&private, &Value::i32(0), &Value::i32(2))))
            } else {
                Some(self.call_v("dream_cycle_construction_begin", &[V::i32(i64::from(policy == crate::AllocPolicy::Private))]))
            }
        } else { None };
        let o = match frame {
            Some(buf) => self.call_v(
                "dream_frame_object",
                &[V::s(buf), V::i64(size as i64), V::i32(tag as i64)],
            ),
            None => {
                let size_tag = [V::i64(size as i64), V::i32(tag as i64)];
                let o = match policy {
                    _ if shared => self.call_v("dream_malloc_shared", &size_tag),
                    crate::AllocPolicy::Private => {
                        let untracked = super::glue::ownership::untracked_info(ty);
                        let [size, tag] = size_tag;
                        self.call_v("dream_malloc_private", &[size, tag, V::s(Value::global(untracked))])
                    }
                    crate::AllocPolicy::Tracked => self.call_v("dream_malloc", &size_tag),
                };
                let p = self.ptr(&o);
                self.memset0(&p, &Value::i64(size as i64));
                o
            }
        };
        if let Some(name) = ctor_name {
            let mut all = vec![o.clone()];
            all.extend(vals);
            if policy == crate::AllocPolicy::Private && let Some(gate) = &gate {
                let private = self.w.new_block("construct.private");
                let tracked = self.w.new_block("construct.tracked");
                let done = self.w.new_block("construct.done");
                let active = self.w.icmp("eq", &gate.v, &Value::i32(0));
                self.w.cond_br(&active, private, tracked);
                self.w.switch_to(private);
                self.call(&super::construction::private_name(&name), &all);
                self.w.br(done);
                self.w.switch_to(tracked);
                self.call(&super::construction::tracked_name(&name), &all);
                self.w.br(done);
                self.w.switch_to(done);
            } else {
                let name = if gate.is_some() { super::construction::tracked_name(&name) } else { name };
                self.call(&name, &all);
            }
        }
        if !borrows_builder_gate && let Some(gate) = gate {
            self.call("dream_cycle_store_end", &[gate]);
        }
        o
    }

    /// Constructs a value struct directly into `dest`: zero it, run the constructor in place.
    pub fn struct_new_at(&mut self, dest: &V, ty: TypeId, ctor: Option<DefId>, args: &[Operand]) {
        let size = self
            .l
            .cx
            .nstruct(ty)
            .unwrap_or_else(|| {
                crate::internal_error!("missing layout for struct allocation {ty:?}")
            })
            .size;
        for a in args {
            self.retain_rc_global_sink(true, a);
        }
        let vals: Vec<V> = args.iter().map(|a| self.operand(a)).collect();
        let ctor_name = self.ctor_name(ctor);
        let p = self.ptr(dest);
        self.memset0(&p, &Value::i64(size as i64));
        if let Some(name) = ctor_name {
            let mut all = vec![dest.clone()];
            all.extend(vals);
            self.call(&name, &all);
        }
    }

    pub fn union_new_at(&mut self, dest: &V, ty: TypeId, variant: usize, args: &[Operand]) {
        let u = self
            .l
            .cx
            .nunion(ty)
            .cloned()
            .unwrap_or_else(|| crate::internal_error!("missing union layout {ty:?}"));
        let var = u
            .variants
            .iter()
            .find(|v| v.discriminant as usize == variant)
            .cloned()
            .unwrap_or_else(|| crate::internal_error!("missing variant {variant}"));
        let p = self.ptr(dest);
        self.memset0(&p, &Value::i64(u.size as i64));
        self.store_ty(&Ty::I32, &p, &V::i32(variant as i64), PAYLOAD_ALIGN);
        for (i, arg) in args.iter().enumerate() {
            let fld = &var.fields[i];
            let at = self.addr(dest, fld.offset as i64);
            if self.is_value(fld.ty) {
                let fsz = elem_size(&self.l.cx, fld.ty) as i64;
                let src = self.operand(arg);
                let sp = self.ptr(&src);
                self.memcpy(&at, &sp, &Value::i64(fsz));
                if !matches!(arg, Operand::Copy(crate::Place::Local(_))) {
                    let atv = self.as_ref(&V::s(at));
                    self.value_refs(fld.ty, &atv, true);
                }
                continue;
            }
            let m = mem_ty(&self.l.cx, fld.ty);
            let val = self.operand(arg);
            let align = super::fx::align_at(&self.h(), fld.offset as i64).min(PAYLOAD_ALIGN);
            self.store_mem(m, &at, &val, align);
            if self.is_rc(fld.ty) {
                let loaded = self.load_mem(m, &at, align);
                let sym = retain_sym(&self.l.cx, fld.ty);
                self.call(sym, &[loaded]);
            }
        }
    }

    fn emit_union_new(&mut self, ty: TypeId, variant: usize, args: &[Operand]) -> V {
        // Construction borrows its payload; a transfer elsewhere in the function does not
        // authorize consuming this generation's token. Niche unions share the payload pointer.
        if self.interner.is_niche_union(ty) {
            let arg = match args {
                [a] => a.clone(),
                _ => return V::s(Value::zero(self.h())),
            };
            let e = self.operand(&arg);
            let payload_ty = self
                .l
                .cx
                .nunion(ty)
                .and_then(|u| u.variants.iter().find(|v| !v.fields.is_empty()))
                .and_then(|v| v.fields.first())
                .map(|f| f.ty);
            let Some(payload_ty) = payload_ty else {
                return e;
            };
            let sym = retain_sym(&self.l.cx, payload_ty);
            self.call(sym, std::slice::from_ref(&e));
            return e;
        }
        let size = self
            .l
            .cx
            .nunion(ty)
            .unwrap_or_else(|| crate::internal_error!("missing union layout {ty:?}"))
            .size;
        let tag = self.l.cx.type_tag(ty);
        let o = self.call_v("dream_malloc", &[V::i64(size as i64), V::i32(tag as i64)]);
        self.union_new_at(&o, ty, variant, args);
        o
    }

    pub(super) fn install_array_info(&mut self, array: &V, elem: TypeId) {
        if crate::backend::shared::glue::glue_array_elems(&self.l.cx).contains(&elem) {
            let info = V::s(Value::global(super::glue::ownership::array_info(elem)));
            self.call("dream_set_type", &[array.clone(), info]);
        }
    }

    fn emit_array_lit(&mut self, elem_ty: TypeId, elems: &[Operand]) -> V {
        let es = elem_size(&self.l.cx, elem_ty) as i64;
        let n = elems.len() as i64;
        let size = 4 + es * n;
        let m = mem_ty(&self.l.cx, elem_ty);
        let vals: Vec<V> = elems.iter().map(|e| self.operand(e)).collect();
        let is_val = self.is_value(elem_ty);
        let rc = self.is_rc(elem_ty);
        let o = self.call_v(
            "dream_malloc",
            &[V::i64(size), V::i32(crate::abi::TAG_ARRAY as i64)],
        );
        let p = self.ptr(&o);
        self.memset0(&p, &Value::i64(size));
        self.install_array_info(&o, elem_ty);
        self.store_ty(&Ty::I32, &p, &V::i32(n), 4);
        for (i, v) in vals.iter().enumerate() {
            let at = self
                .w
                .gep_const(&p, crate::abi::LEN_PREFIX_SIZE as i64 + i as i64 * es);
            if is_val {
                let sp = self.ptr(v);
                self.memcpy(&at, &sp, &Value::i64(es));
                let atv = self.as_ref(&V::s(at));
                self.publish_refs(elem_ty, &atv, &o);
                self.value_refs(elem_ty, &atv, true);
            } else {
                self.store_mem(m, &at, v, PAYLOAD_ALIGN);
                if rc {
                    let loaded = self.load_mem(m, &at, PAYLOAD_ALIGN);
                    if crate::ownership::contains_cycle_refs(&self.mir.layouts, self.interner, elem_ty) {
                        self.call("dream_cycle_check_store", &[o.clone(), loaded.clone()]);
                    }
                    let sym = retain_sym(&self.l.cx, elem_ty);
                    self.call(sym, &[loaded]);
                }
            }
        }
        o
    }

    fn emit_tuple(&mut self, ty: TypeId, elems: &[Operand]) -> V {
        let layout = self
            .l
            .cx
            .nstruct(ty)
            .cloned()
            .unwrap_or_else(|| crate::internal_error!("missing tuple layout {ty:?}"));
        let size = layout.size.max(1) as i64;
        let tag = self.l.cx.type_tag(ty);
        let vals: Vec<V> = elems.iter().map(|e| self.operand(e)).collect();
        let o = self.call_v("dream_malloc", &[V::i64(size), V::i32(tag as i64)]);
        let p = self.ptr(&o);
        self.memset0(&p, &Value::i64(size));
        for (i, v) in vals.iter().enumerate() {
            let Some(fld) = layout.fields.get(i) else {
                continue;
            };
            let at = self.w.gep_const(&p, fld.offset as i64);
            if self.is_value(fld.ty) {
                let sz = elem_size(&self.l.cx, fld.ty) as i64;
                let sp = self.ptr(v);
                self.memcpy(&at, &sp, &Value::i64(sz));
                continue;
            }
            let m = mem_ty(&self.l.cx, fld.ty);
            let align = super::fx::align_at(&self.h(), fld.offset as i64);
            self.store_mem(m, &at, v, align);
            if self.is_rc(fld.ty) {
                let loaded = self.load_mem(m, &at, align);
                let sym = retain_sym(&self.l.cx, fld.ty);
                self.call(sym, &[loaded]);
            }
        }
        o
    }

    /// A union value's discriminant. A niche union carries no tag word: nullness recovers it.
    pub fn union_discriminant(&mut self, ty: TypeId, base: &V) -> V {
        if let Some((some_disc, none_disc)) = self.l.cx.niche_variant_discriminants(ty) {
            let nz = self.truthy(base);
            return V::s(self.w.select(
                &nz,
                &Value::i32(some_disc as i64),
                &Value::i32(none_disc as i64),
            ));
        }
        let p = self.ptr(base);
        self.load_ty(Ty::I32, &p, PAYLOAD_ALIGN, false)
    }

    /// Field `field` of variant `variant` of a union value.
    pub fn union_field(&mut self, ty: TypeId, variant: usize, field: usize, base: &V) -> V {
        if self.interner.is_niche_union(ty) {
            return base.clone();
        }
        let u = self
            .l
            .cx
            .nunion(ty)
            .unwrap_or_else(|| crate::internal_error!("missing union layout for {ty:?}"));
        let var = u
            .variants
            .iter()
            .find(|v| v.discriminant as usize == variant)
            .unwrap_or_else(|| crate::internal_error!("missing union variant {variant}"));
        let fld = var
            .fields
            .get(field)
            .cloned()
            .unwrap_or_else(|| crate::internal_error!("missing union field {field}"));
        let at = self.addr(base, fld.offset as i64);
        if self.is_value(fld.ty) {
            return self.as_ref(&V::s(at));
        }
        let m = mem_ty(&self.l.cx, fld.ty);
        let align = super::fx::align_at(&self.h(), fld.offset as i64).min(PAYLOAD_ALIGN);
        self.load_mem(m, &at, align)
    }

    pub fn emit_cast(&mut self, v: &Operand, from: TypeId, to: TypeId) -> V {
        let src = self.operand(v);
        if from == to {
            return src;
        }
        if let Some(r) = self.js_cast(&src, from, to) {
            return r;
        }
        let fk = self.interner.kind(from).clone();
        let tk = self.interner.kind(to).clone();
        if let (TyKind::Prim(from_prim), TyKind::Prim(to_prim)) = (&fk, &tk)
            && from_prim.is_numeric()
            && to_prim.is_numeric()
            && (matches!(from_prim, PrimTy::ISize | PrimTy::USize)
                || matches!(to_prim, PrimTy::ISize | PrimTy::USize))
        {
            let source_ty = super::types::ll_ty(self.interner, from, &self.h(), &self.word());
            let target_ty = super::types::ll_ty(self.interner, to, &self.h(), &self.word());
            let source = self.conv_v(&src, &source_ty, from_prim.is_unsigned_integer());
            return self.conv_v(&source, &target_ty, to_prim.is_unsigned_integer());
        }
        if matches!(tk, TyKind::Object | TyKind::Interface(..)) && self.is_value(from) {
            let size = elem_size(&self.l.cx, from) as i64;
            let tag = self.l.cx.type_tag(from);
            let b = self.call_v("dream_malloc", &[V::i64(size), V::i32(tag as i64)]);
            let (bp, sp) = (self.ptr(&b), self.ptr(&src));
            self.memcpy(&bp, &sp, &Value::i64(size));
            return b;
        }
        let i32s = |fx: &mut Self, x: &V| fx.conv_v(x, &Ty::I32, false);
        let u32s = |fx: &mut Self, x: &V| fx.conv_v(x, &Ty::I32, true);
        use PrimTy as P;
        match (&fk, &tk) {
            (TyKind::Prim(P::Int), TyKind::Prim(P::Long)) => {
                let x = i32s(self, &src);
                self.conv_v(&x, &Ty::I64, false)
            }
            (TyKind::Prim(P::Int | P::Byte | P::Char), TyKind::Prim(P::Double)) => {
                let x = i32s(self, &src);
                self.conv_v(&x, &Ty::F64, false)
            }
            (TyKind::Prim(P::Int | P::Byte | P::Char), TyKind::Prim(P::Float)) => {
                let x = i32s(self, &src);
                self.conv_v(&x, &Ty::F32, false)
            }
            (TyKind::Prim(P::UInt), TyKind::Prim(P::Double)) => {
                let x = u32s(self, &src);
                self.conv_v(&x, &Ty::F64, false)
            }
            (TyKind::Prim(P::UInt), TyKind::Prim(P::Float)) => {
                let x = u32s(self, &src);
                self.conv_v(&x, &Ty::F32, false)
            }
            (TyKind::Prim(P::UInt), TyKind::Prim(P::Long)) => {
                let x = u32s(self, &src);
                self.conv_v(&x, &Ty::I64, false)
            }
            (TyKind::Prim(P::ULong), TyKind::Prim(P::Double)) => {
                let x = self.conv_v(&src, &Ty::I64, true);
                self.conv_v(&x, &Ty::F64, false)
            }
            (TyKind::Prim(P::ULong), TyKind::Prim(P::Float)) => {
                let x = self.conv_v(&src, &Ty::I64, true);
                self.conv_v(&x, &Ty::F32, false)
            }
            (TyKind::Prim(P::Float), TyKind::Prim(P::Double)) => {
                let x = self.conv_v(&src, &Ty::F32, false);
                self.conv_v(&x, &Ty::F64, false)
            }
            (TyKind::Prim(P::Double), TyKind::Prim(P::Float)) => self.conv_v(&src, &Ty::F32, false),
            (TyKind::Prim(P::Double | P::Float | P::Long), TyKind::Prim(P::Int)) => {
                i32s(self, &src)
            }
            (_, TyKind::Object) => {
                if matches!(fk, TyKind::Prim(P::ISize | P::USize)) {
                    let size = elem_size(&self.l.cx, from) as i64;
                    let tag = runtime_tag(&self.l.cx, from);
                    let boxed = self.call_v("dream_malloc", &[V::i64(size), V::i32(tag as i64)]);
                    let address = self.ptr(&boxed);
                    self.store_ty(&self.word(), &address, &src, size as u32);
                    return boxed;
                }
                let (f, t) = match fk {
                    TyKind::Prim(P::Int) => ("dream_box_int", Ty::I32),
                    TyKind::Prim(P::Float) => ("dream_box_float", Ty::F32),
                    TyKind::Prim(P::Double) => ("dream_box_double", Ty::F64),
                    TyKind::Prim(P::Bool) => ("dream_box_bool", Ty::I32),
                    TyKind::Prim(P::Char) => ("dream_box_char", Ty::I32),
                    TyKind::Prim(P::Long) => ("dream_box_long", Ty::I64),
                    TyKind::Prim(P::UInt) => ("dream_box_uint", Ty::I32),
                    TyKind::Prim(P::ULong) => ("dream_box_ulong", Ty::I64),
                    TyKind::Prim(P::Byte) => ("dream_box_byte", Ty::I32),
                    _ => return src,
                };
                let x = self.conv_v(&src, &t, false);
                self.call_v(f, &[x])
            }
            (TyKind::Object, TyKind::Prim(p)) => {
                let unbox = match p {
                    P::Int => "dream_unbox_int",
                    P::Float => "dream_unbox_float",
                    P::Double => "dream_unbox_double",
                    P::Bool => "dream_unbox_bool",
                    P::Char => "dream_unbox_char",
                    P::Long => "dream_unbox_long",
                    P::UInt => "dream_unbox_uint",
                    P::ULong => "dream_unbox_ulong",
                    P::ISize if self.l.cx.mir.layouts.target.ptr_size == 8 => "dream_unbox_long",
                    P::USize if self.l.cx.mir.layouts.target.ptr_size == 8 => "dream_unbox_ulong",
                    P::ISize => "dream_unbox_int",
                    P::USize => "dream_unbox_uint",
                    P::Byte => "dream_unbox_byte",
                    P::String => return src,
                };
                self.checked_unbox(&src, to, unbox)
            }
            _ => src,
        }
    }

    fn checked_unbox(&mut self, src: &V, ty: TypeId, unbox: &str) -> V {
        let tag = runtime_tag(&self.l.cx, ty);
        let boxed = self.as_ref(src);
        let t = self.call_v("dream_object_tag", std::slice::from_ref(&boxed));
        let t = self.conv(&t, &Ty::I32);
        let bad = self.w.icmp("ne", &t, &Value::i32(tag as i64));
        self.if_then(&bad, |fx| {
            fx.panic_with(crate::backend::shared::panic_msgs::INVALID_CAST)
        });
        self.call_v(unbox, &[boxed])
    }
}

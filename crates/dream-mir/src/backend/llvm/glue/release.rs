//! ARC glue bodies: `release_*` (null check + decrement wrapper around the `_into` tail),
//! `destroy_*` (unique teardown), and the tag-dispatched `object` routers. Which symbol each type
//! uses and each field's teardown come from `shared::glue`.

use super::super::fx::{Fx, V};
use super::super::ir::{Ty, Value};
use super::super::lcx::Lcx;
use super::super::places::ELEM_ALIGN;
use super::{glue, register};
use crate::abi::{TAG_CLOSURE_ENV, TAG_FUNCBOX, TAG_STRING};
use crate::backend::shared::abi_types::{c_ident, elem_size};
use crate::backend::shared::glue::{
    FieldDrop, del_symbol, destroy_sym, drop_nonempty, field_drop, glue_array_elems, release_sym,
    self_tail_field, struct_field_drops,
};
use dream_hir::TypeLayout;
use dream_types::TypeId;
use std::collections::BTreeSet;

struct Names {
    elems: BTreeSet<TypeId>,
    structs: Vec<(TypeId, TypeLayout)>,
    unions: Vec<(TypeId, dream_hir::UnionLayout)>,
    rel_skip: Vec<TypeId>,
    des_skip: Vec<TypeId>,
}

fn names(l: &Lcx<'_>) -> Names {
    let canon = l.cx.canon_maps();
    Names {
        elems: glue_array_elems(&l.cx),
        structs: l
            .cx
            .mir
            .layouts
            .structs
            .iter()
            .map(|(t, s)| (*t, s.clone()))
            .collect(),
        unions: l
            .cx
            .mir
            .layouts
            .unions
            .iter()
            .map(|(t, u)| (*t, u.clone()))
            .collect(),
        rel_skip: canon.release.keys().copied().collect(),
        des_skip: canon.destroy.keys().copied().collect(),
    }
}

fn arr_rel(e: TypeId) -> String {
    c_ident(&format!("release_array_t{}", e.0))
}

fn arr_des(e: TypeId) -> String {
    c_ident(&format!("destroy_array_t{}", e.0))
}

pub(in super::super) fn register_all(l: &mut Lcx<'_>) {
    let n = names(l);
    let add = |l: &mut Lcx<'_>, name: String| {
        let h = l.h();
        register(l, &name, Ty::Void, vec![h])
    };
    for e in &n.elems {
        add(l, arr_rel(*e));
        add(l, format!("{}_into", arr_rel(*e)));
        add(l, arr_des(*e));
    }
    for (ty, name) in n
        .structs
        .iter()
        .map(|(t, s)| (t, &s.name))
        .chain(n.unions.iter().map(|(t, u)| (t, &u.name)))
    {
        if !n.rel_skip.contains(ty) {
            add(l, c_ident(&format!("release_{name}")));
            add(l, c_ident(&format!("release_{name}_into")));
        }
        if !n.des_skip.contains(ty) {
            add(l, c_ident(&format!("destroy_{name}")));
        }
    }
    add(l, c_ident("destroy_object"));
    add(l, "dream_release_object".into());
    if l.mir.uses_defer {
        add(l, "release_string".into());
        add(l, "destroy_string".into());
    }
}

impl<'l, 'a> Fx<'l, 'a> {
    /// Claiming the last count closes the weak-retain race before entering destroy glue.
    fn drop_rc_slot(&mut self, ty: TypeId, c: &V) {
        let rel = release_sym(&self.l.cx, ty);
        let des = destroy_sym(&self.l.cx, ty);
        // Erased-type destruction routes some runtime tags through a decrementing release.
        // Its count must remain unclaimed until that dispatcher chooses the concrete cascade.
        if rel == des || des == c_ident("destroy_object") {
            self.call(&rel, std::slice::from_ref(c));
            return;
        }
        let nz = self.truthy(c);
        self.if_then(&nz, |fx| {
            let one = fx.call_v("dream_rc_claim_unique", std::slice::from_ref(c));
            let one = fx.truthy(&one);
            let (tb, eb, join) = (
                fx.w.new_block("one"),
                fx.w.new_block("shared"),
                fx.w.new_block("dropped"),
            );
            fx.w.cond_br(&one, tb, eb);
            fx.w.switch_to(tb);
            fx.call(&des, std::slice::from_ref(c));
            fx.w.br(join);
            fx.w.switch_to(eb);
            fx.call(&rel, std::slice::from_ref(c));
            fx.w.br(join);
            fx.w.switch_to(join);
        });
    }

    fn maybe_defer(&mut self, p: &V, destroy: &str) {
        let current = self.w.name().to_string();
        let function = V::u(self.l.fn_ref(&current));
        let queued = self.call_v("dream_cycle_defer_destroy", &[p.clone(), function]);
        let queued = self.truthy(&queued);
        self.if_then(&queued, |fx| fx.w.ret(None));
        if !self.mir.uses_defer {
            return;
        }
        let (g, ty) = self.l.rt_global("dream_defer_open");
        let open = self.load_ty(ty, &g, 4, false);
        let open = self.truthy(&open);
        let destroy = destroy.to_string();
        self.if_then(&open, |fx| {
            let f = fx.l.fn_ref(&destroy);
            let f = V::u(f);
            let q = fx.call_v("dream_defer_try_enqueue", &[p.clone(), f]);
            let q = fx.truthy(&q);
            fx.if_then(&q, |fx| fx.w.ret(None));
        });
    }

    fn del_call(&mut self, p: &V, destructor: Option<dream_types::DefId>) {
        self.call("dream_weak_prepare_destroy", std::slice::from_ref(p));
        if let Some(def) = destructor {
            let del = del_symbol(&self.l.cx, def);
            self.call(&del, std::slice::from_ref(p));
        }
    }

    fn field_word(&mut self, p: &V, offset: u32) -> V {
        let at = self.addr(p, offset as i64);
        self.load_ty(self.h(), &at, ELEM_ALIGN, true)
    }

    pub(super) fn field_drop_code(&mut self, p: &V, d: FieldDrop) {
        match d {
            FieldDrop::None => {}
            FieldDrop::Unregister { offset } => {
                let cur = self.field_word(p, offset);
                let nz = self.truthy(&cur);
                self.if_then(&nz, |fx| {
                    let at = fx.addr(p, offset as i64);
                    let at = fx.as_ref(&V::s(at));
                    fx.call("dream_weak_unregister", &[cur.clone(), at]);
                });
            }
            FieldDrop::Value { offset, ty } => {
                let at = self.addr(p, offset as i64);
                let at = self.as_ref(&V::s(at));
                self.value_refs(ty, &at, false);
            }
            FieldDrop::Rc { offset, ty } => {
                let c = self.field_word(p, offset);
                self.drop_rc_slot(ty, &c);
            }
        }
    }

    /// Both arms claim the last reference; a count peek would let weak loads resurrect it.
    fn unique_or_last(&mut self, c: &V) -> Value {
        let nz = self.truthy(c);
        let yes = self.if_else_v(
            &nz,
            |fx| {
                let one = fx.call_v("dream_rc_claim_unique", std::slice::from_ref(c));
                let one = fx.truthy(&one);
                let one = V::s(one);
                let o = one.v.clone();
                fx.if_else_v(
                    &o,
                    |_| V::s(Value::i1(true)),
                    |fx| {
                        let last = fx.call_v("dream_rc_last", std::slice::from_ref(c));
                        V::s(fx.truthy(&last))
                    },
                )
            },
            |_| V::s(Value::i1(false)),
        );
        yes.v
    }

    pub(super) fn array_elems_drop(&mut self, p: &V, elem: TypeId) {
        let es = elem_size(&self.l.cx, elem) as i64;
        let value = self.is_value(elem);
        if !value && !self.is_rc(elem) {
            return;
        }
        let pp = self.ptr(p);
        let n = self.load_ty(Ty::I32, &pp, 4, false);
        let n = self.conv(&n, &Ty::I64);
        let slot = self.w.alloca(Ty::I64, 8);
        self.w.store(&Value::i64(0), &slot, 8, &[]);
        let head = self.w.new_block("arr.head");
        let body = self.w.new_block("arr.body");
        let done = self.w.new_block("arr.done");
        self.w.br(head);
        self.w.switch_to(head);
        let i = self.w.load(Ty::I64, &slot, 8, &[]);
        let lt = self.w.icmp("slt", &i, &n);
        self.w.cond_br(&lt, body, done);
        self.w.switch_to(body);
        let off = self.w.bin("mul", &i, &Value::i64(es));
        let off = self.w.bin("add", &off, &Value::i64(4));
        let at = self.addr_dyn(p, &off);
        if value {
            let at = self.as_ref(&V::s(at));
            self.value_refs(elem, &at, false);
        } else {
            let c = self.load_ty(self.h(), &at, ELEM_ALIGN, true);
            self.drop_rc_slot(elem, &c);
        }
        let inc = self.w.bin("add", &i, &Value::i64(1));
        self.w.store(&inc, &slot, 8, &[]);
        self.w.br(head);
        self.w.switch_to(done);
    }

    fn rc_last_or_ret(&mut self, p: &V) {
        let last = self.call_v("dream_rc_last", std::slice::from_ref(p));
        let last = self.truthy(&last);
        let not = self.w.bin("xor", &last, &Value::i1(true));
        self.if_then(&not, |fx| fx.w.ret(None));
    }

    fn immortal_ret(&mut self, p: &V) {
        let imm = self.call_v("dream_rc_immortal", std::slice::from_ref(p));
        let imm = self.truthy(&imm);
        self.if_then(&imm, |fx| fx.w.ret(None));
    }

    pub(super) fn union_drops(&mut self, p: &V, u: &dream_hir::UnionLayout) {
        let pp = self.ptr(p);
        let disc = self.load_ty(Ty::I32, &pp, 4, false);
        let join = self.w.new_block("u.join");
        let mut arms = Vec::new();
        let mut bodies = Vec::new();
        for v in &u.variants {
            let drops: Vec<FieldDrop> = v
                .fields
                .iter()
                .map(|f| field_drop(&self.l.cx, f, true))
                .collect();
            if drops.iter().all(|d| !drop_nonempty(&self.l.cx, *d)) {
                continue;
            }
            let b = self.w.new_block("u.arm");
            arms.push((v.discriminant as i128, b));
            bodies.push((b, drops));
        }
        self.w.switch(&disc.v, join, &arms);
        for (b, drops) in bodies {
            self.w.switch_to(b);
            for d in drops {
                self.field_drop_code(p, d);
            }
            self.w.br(join);
        }
        self.w.switch_to(join);
    }
}

fn tail_of(l: &Lcx<'_>, layout: &TypeLayout, destroy: &str) -> Option<usize> {
    let has: Vec<bool> = struct_field_drops(&l.cx, layout)
        .into_iter()
        .map(|d| drop_nonempty(&l.cx, d))
        .collect();
    self_tail_field(&l.cx, layout, &has, destroy)
}

/// `release_X`: `if (!p || !dream_rc_last(p)) return; release_X_into(p);`
fn wrapper(l: &mut Lcx<'_>, name: &str, tail: &str) {
    let mut fx = glue(l, name);
    let p = fx.arg(0);
    fx.ret_if_null(&p);
    fx.rc_last_or_ret(&p);
    fx.call(tail, &[p]);
    fx.w.ret(None);
    fx.finish();
}

pub(in super::super) fn emit_all(l: &mut Lcx<'_>) {
    let n = names(l);
    for e in &n.elems {
        let tail = format!("{}_into", arr_rel(*e));
        let mut fx = glue(l, &tail);
        let p = fx.arg(0);
        fx.maybe_defer(&p, &arr_des(*e));
        fx.array_elems_drop(&p, *e);
        fx.call("dream_recycle", &[p]);
        fx.w.ret(None);
        fx.finish();
        wrapper(l, &arr_rel(*e), &tail);
    }
    for (ty, layout) in &n.structs {
        if n.rel_skip.contains(ty) {
            continue;
        }
        let tail_name = c_ident(&format!("release_{}_into", layout.name));
        let destroy = destroy_sym(&l.cx, *ty);
        let drops = struct_field_drops(&l.cx, layout);
        let tail = tail_of(l, layout, &destroy);
        let mut fx = glue(l, &tail_name);
        let p = fx.arg(0);
        fx.maybe_defer(&p, &c_ident(&format!("destroy_{}", layout.name)));
        fx.del_call(&p, layout.destructor);
        for (i, d) in drops.into_iter().enumerate() {
            if Some(i) != tail {
                fx.field_drop_code(&p, d);
            }
        }
        if let Some(i) = tail {
            let next = fx.field_word(&p, layout.fields[i].offset);
            fx.call("dream_recycle", &[p]);
            let go = fx.unique_or_last(&next);
            fx.if_then(&go, |fx| {
                fx.call(&destroy, std::slice::from_ref(&next));
            });
        } else {
            fx.call("dream_recycle", &[p]);
        }
        fx.w.ret(None);
        fx.finish();
        wrapper(l, &c_ident(&format!("release_{}", layout.name)), &tail_name);
    }
    for (ty, layout) in &n.unions {
        if n.rel_skip.contains(ty) {
            continue;
        }
        let tail_name = c_ident(&format!("release_{}_into", layout.name));
        let mut fx = glue(l, &tail_name);
        let p = fx.arg(0);
        fx.maybe_defer(&p, &c_ident(&format!("destroy_{}", layout.name)));
        fx.union_drops(&p, layout);
        fx.call("dream_recycle", &[p]);
        fx.w.ret(None);
        fx.finish();
        wrapper(l, &c_ident(&format!("release_{}", layout.name)), &tail_name);
    }
    if l.mir.uses_defer {
        let mut fx = glue(l, "release_string");
        let p = fx.arg(0);
        fx.ret_if_null(&p);
        fx.rc_last_or_ret(&p);
        fx.maybe_defer(&p, "destroy_string");
        fx.call("dream_free", &[p]);
        fx.w.ret(None);
        fx.finish();
        let mut fx = glue(l, "destroy_string");
        let p = fx.arg(0);
        fx.ret_if_null(&p);
        fx.call("dream_free", &[p]);
        fx.w.ret(None);
        fx.finish();
    }
    emit_destroys(l, &n);
    tag_dispatch(l, &c_ident("destroy_object"), true);
    tag_dispatch(l, "dream_release_object", false);
}

fn emit_destroys(l: &mut Lcx<'_>, n: &Names) {
    for e in &n.elems {
        let name = arr_des(*e);
        let mut fx = glue(l, &name);
        let p = fx.arg(0);
        fx.ret_if_null(&p);
        fx.immortal_ret(&p);
        fx.maybe_defer(&p, &name);
        fx.array_elems_drop(&p, *e);
        fx.call("dream_recycle", &[p]);
        fx.w.ret(None);
        fx.finish();
    }
    for (ty, layout) in &n.structs {
        if n.des_skip.contains(ty) {
            continue;
        }
        let name = c_ident(&format!("destroy_{}", layout.name));
        let drops = struct_field_drops(&l.cx, layout);
        let tail = tail_of(l, layout, &name);
        let mut fx = glue(l, &name);
        let p0 = fx.arg(0);
        // A self-typed chain is torn down by looping instead of recursing, so dropping a long
        // list uses O(1) stack; dropping the tail last keeps every `del` in field order.
        let lp = tail.map(|_| {
            let slot = fx.w.alloca(fx.h(), 8);
            fx.w.store(&p0.v, &slot, 8, &[]);
            let again = fx.w.new_block("again");
            fx.w.br(again);
            fx.w.switch_to(again);
            (slot, again)
        });
        let p = match &lp {
            Some((slot, _)) => V::u(fx.w.load(fx.h(), slot, 8, &[])),
            None => p0,
        };
        fx.ret_if_null(&p);
        fx.immortal_ret(&p);
        fx.maybe_defer(&p, &name);
        fx.del_call(&p, layout.destructor);
        for (i, d) in drops.into_iter().enumerate() {
            if Some(i) != tail {
                fx.field_drop_code(&p, d);
            }
        }
        match tail {
            Some(i) => {
                let next = fx.field_word(&p, layout.fields[i].offset);
                fx.call("dream_recycle", std::slice::from_ref(&p));
                let go = fx.unique_or_last(&next);
                let (slot, again) = lp
                    .clone()
                    .unwrap_or_else(|| crate::internal_error!("destroy loop without a slot"));
                let out = fx.w.new_block("out");
                fx.w.store(&next.v, &slot, 8, &[]);
                fx.w.cond_br(&go, again, out);
                fx.w.switch_to(out);
            }
            None => {
                fx.call("dream_recycle", &[p]);
            }
        }
        fx.w.ret(None);
        fx.finish();
    }
    for (ty, layout) in &n.unions {
        if n.des_skip.contains(ty) {
            continue;
        }
        let name = c_ident(&format!("destroy_{}", layout.name));
        let mut fx = glue(l, &name);
        let p = fx.arg(0);
        fx.ret_if_null(&p);
        fx.immortal_ret(&p);
        fx.maybe_defer(&p, &name);
        fx.union_drops(&p, layout);
        fx.call("dream_recycle", &[p]);
        fx.w.ret(None);
        fx.finish();
    }
}

fn tag_dispatch(l: &mut Lcx<'_>, name: &str, destroy: bool) {
    let mut arms: Vec<(i128, String)> = Vec::new();
    let tagged =
        l.cx.mir
            .layouts
            .structs
            .keys()
            .chain(l.cx.mir.layouts.unions.keys())
            .copied()
            .collect::<Vec<_>>();
    for ty in tagged {
        if let Some(&tag) = l.cx.tags.get(&ty) {
            let sym = if destroy {
                destroy_sym(&l.cx, ty)
            } else {
                release_sym(&l.cx, ty)
            };
            if !arms.iter().any(|(t, _)| *t == tag as i128) {
                arms.push((tag as i128, sym));
            }
        }
    }
    let env = l.interner.object();
    for (tag, sym) in [
        (TAG_STRING as i128, "dream_release".to_string()),
        (TAG_FUNCBOX as i128, "dream_release_funcbox".to_string()),
        (
            TAG_CLOSURE_ENV as i128,
            if destroy { arr_des(env) } else { arr_rel(env) },
        ),
    ] {
        if !arms.iter().any(|(t, _)| *t == tag) {
            arms.push((tag, sym));
        }
    }
    let mut fx = glue(l, name);
    let p = fx.arg(0);
    fx.ret_if_null(&p);
    if !destroy {
        let release_extra = |fx: &mut Fx<'_, '_>| {
            let released = fx.call_v("dream_release_nonlast", std::slice::from_ref(&p));
            let released = fx.truthy(&released);
            fx.if_then(&released, |fx| fx.w.ret(None));
        };
        if fx.mir.uses_defer {
            let (g, ty) = fx.l.rt_global("dream_defer_open");
            let open = fx.load_ty(ty, &g, 4, false);
            let closed = fx.w.icmp("eq", &open.v, &Value::zero(open.ty().clone()));
            fx.if_then(&closed, release_extra);
        } else { release_extra(&mut fx); }
    }
    fx.maybe_defer(&p, name);
    let tag = fx.call_v("dream_object_tag", std::slice::from_ref(&p));
    let tag = fx.conv(&tag, &Ty::I32);
    let default = fx.w.new_block("other");
    let blocks: Vec<_> = arms.iter().map(|_| fx.w.new_block("tag")).collect();
    let cases: Vec<(i128, _)> = arms
        .iter()
        .zip(&blocks)
        .map(|((t, _), b)| (*t, *b))
        .collect();
    fx.w.switch(&tag, default, &cases);
    for ((_, sym), b) in arms.iter().zip(blocks) {
        fx.w.switch_to(b);
        fx.call(sym, std::slice::from_ref(&p));
        fx.w.ret(None);
    }
    fx.w.switch_to(default);
    fx.call("dream_release", &[p]);
    fx.w.ret(None);
    fx.finish();
}

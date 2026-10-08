//! Function bodies: a sync function, and an async function's (stub, poll, drop) triple. Every MIR
//! block becomes one LLVM block; locals are entry allocas that mem2reg promotes.

use super::fx::{Fx, Slot, V, align_at};
use super::ir::{BlockRef, FnAttr, Ty, Value};
use super::lcx::Lcx;
use super::types::is_unsigned;
use crate::backend::shared::abi_types::native_scalar_size;
use crate::backend::shared::glue::{release_sym, retain_sym};
use crate::backend::shared::place_policy::has_frame_buffer;
use crate::{Local, MirFunction, Operand, Place, Statement, Terminator};
use dream_types::{PrimTy, TyKind};

impl<'l, 'a> Fx<'l, 'a> {
    pub fn frame_buf(&self, l: Local) -> Value {
        self.frame_bufs
            .get(l.0 as usize)
            .cloned()
            .flatten()
            .unwrap_or_else(|| crate::internal_error!("local {} has no frame buffer", l.0))
    }

    fn scalar_slot(&mut self, l: Local) -> Slot {
        let ty = self.local_ll(l);
        let ptr = self.w.alloca(ty.clone(), 8);
        Slot {
            ptr,
            ty,
            unsigned: is_unsigned(self.interner, self.f.local_ty(l)),
        }
    }

    fn value_buf(&mut self, size: u32) -> Value {
        self.alloca_bytes(size.max(1) as u64, 8)
    }

    fn map_blocks(&mut self) {
        self.blocks = (0..self.f.blocks.len())
            .map(|i| self.w.new_block(&format!("L{i}")))
            .collect();
    }

    fn frame_buffers(&mut self) {
        let f = self.f;
        self.frame_bufs = vec![None; f.locals.len()];
        for (i, decl) in f.locals.iter().enumerate() {
            if !has_frame_buffer(self.mir, f, Local(i as u32)) {
                continue;
            }
            let size = self.l.cx.nstruct(decl.ty).map_or(0, |l| l.size);
            let total = self.l.cx.target.abi().heap_header_size + size;
            self.frame_bufs[i] = Some(self.alloca_bytes(total as u64, 16));
        }
    }

    /// Sync prologue: params into slots (value params get a private copy unless borrowed), every
    /// other local zeroed, value locals pointed at their own zeroed buffer.
    fn sync_locals(&mut self) {
        let f = self.f;
        self.slots = (0..f.locals.len()).map(|_| None).collect();
        for (pi, p) in f.params.iter().enumerate() {
            let ty = f.local_ty(*p);
            let decl = &f.locals[p.0 as usize];
            let slot = self.scalar_slot(*p);
            let arg = V {
                v: self.w.param(pi),
                unsigned: slot.unsigned,
            };
            self.store_ty(&slot.ty.clone(), &slot.ptr.clone(), &arg, 8);
            self.slots[p.0 as usize] = Some(slot);
            if !self.is_value(ty) || decl.is_ref || decl.name.as_deref() == Some("this") {
                continue;
            }
            let size = native_scalar_size(&self.l.cx, ty).0.max(1);
            let buf = self.value_buf(size);
            let src = self.ptr(&arg);
            self.memcpy(&buf, &src, &Value::i64(size as i64));
            let r = self.as_ref(&V::s(buf));
            self.write_local(*p, &r);
        }
        for (i, decl) in f.locals.iter().enumerate() {
            let l = Local(i as u32);
            if f.params.contains(&l) || matches!(self.interner.kind(decl.ty), TyKind::Void) {
                continue;
            }
            let slot = self.scalar_slot(l);
            let (p, t) = (slot.ptr.clone(), slot.ty.clone());
            self.slots[i] = Some(slot);
            if self.is_value(decl.ty) {
                let size = native_scalar_size(&self.l.cx, decl.ty).0.max(1);
                let buf = self.value_buf(size);
                self.memset0(&buf, &Value::i64(size as i64));
                let r = self.as_ref(&V::s(buf));
                self.store_ty(&t, &p, &r, 8);
            } else {
                self.w.store(&Value::zero(t), &p, 8, &[]);
            }
        }
    }

    fn body_blocks(&mut self, resume_dest: &[Option<u32>], spill: Option<&[i32]>) {
        let f = self.f;
        for (bi, block) in f.blocks.iter().enumerate() {
            self.w.switch_to(self.blocks[bi]);
            self.source_block(bi);
            if let Some(d) = resume_dest.get(bi).copied().flatten() {
                self.await_handoff(Local(d));
            }
            self.stmts(&block.stmts);
            if let Some(offs) = spill {
                self.spill_dirty(bi, resume_dest, offs);
            }
            if !self.w.is_terminated() {
                self.term(&block.terminator);
            }
        }
    }
}

fn inline_attr(f: &MirFunction, preserve_frames: bool) -> Option<FnAttr> {
    if preserve_frames {
        return Some(FnAttr::NoInline);
    }
    match f.inline {
        dream_hir::InlineHint::Never => Some(FnAttr::NoInline),
        dream_hir::InlineHint::Prefer => Some(FnAttr::AlwaysInline),
        dream_hir::InlineHint::Default => None,
    }
}

pub(super) fn build_sync<'a>(
    l: &mut Lcx<'a>,
    f: &'a MirFunction,
    mode: super::construction::InitMode,
) {
    let name = l.user_fn(f);
    let name = match mode {
        super::construction::InitMode::Ordinary => name,
        super::construction::InitMode::Private => super::construction::private_name(&name),
        super::construction::InitMode::Tracked => super::construction::tracked_name(&name),
        super::construction::InitMode::PrivateBuilder => super::construction::builder_name(&name),
        super::construction::InitMode::TrackedBuilder => {
            super::construction::tracked_builder_name(&name)
        }
    };
    let mut w = l.writer(&name);
    w.attrs
        .extend(inline_attr(f, l.cx.leak_checks && l.cx.debug_syms));
    let mut fx = Fx::new(l, f, w);
    fx.private_init = matches!(mode, super::construction::InitMode::Private);
    fx.tracked_init = matches!(mode, super::construction::InitMode::Tracked);
    fx.private_builder = matches!(mode, super::construction::InitMode::PrivateBuilder);
    fx.tracked_builder = matches!(mode, super::construction::InitMode::TrackedBuilder);
    fx.debug_begin(f);
    fx.source_begin(f);
    fx.sync_locals();
    if fx.private_builder {
        fx.construction_gate = Some(V::i32(0));
    } else if fx.tracked_builder {
        fx.construction_gate = Some(V::i32(2));
    } else if let Some(policy) = f.batched_construction {
        fx.debug_locals(true);
        let private = i64::from(policy == crate::AllocPolicy::Private);
        let grouped = super::construction::only_fresh_edges(fx.mir, f);
        let begin = if grouped {
            "dream_cycle_graph_begin"
        } else {
            "dream_cycle_construction_begin"
        };
        let end = if grouped {
            "dream_cycle_graph_end"
        } else {
            "dream_cycle_store_end"
        };
        let gate = fx.call_v(begin, &[V::i32(private)]);
        if policy == crate::AllocPolicy::Private {
            let active = fx.w.icmp("eq", &gate.v, &Value::i32(0));
            fx.if_then(&active, |fx| {
                let name = super::construction::builder_name(&fx.l.user_fn(f));
                let arguments: Vec<_> = f.params.iter().map(|p| fx.read_local(*p)).collect();
                let result = fx.call_v(&name, &arguments);
                fx.w.ret(Some(&result.v));
            });
        }
        let name = super::construction::tracked_builder_name(&fx.l.user_fn(f));
        let arguments: Vec<_> = f.params.iter().map(|p| fx.read_local(*p)).collect();
        let result = fx.call_v(&name, &arguments);
        fx.call(end, &[gate]);
        fx.w.ret(Some(&result.v));
        fx.finish();
        return;
    }
    fx.map_blocks();
    fx.frame_buffers();
    fx.debug_locals(true);
    let entry = fx.blocks[f.entry.0 as usize];
    fx.w.br(entry);
    fx.body_blocks(&[], None);
    let w = fx.w;
    l.define(w);
}

/// A sync function whose value-struct result goes into a caller buffer. Exported names keep the
/// box ABI the runtime and C callers expect.
pub(super) fn returns_via_buffer(l: &Lcx<'_>, f: &MirFunction, name: &str) -> bool {
    !f.is_async && name != "main" && !l.sigs.has_function(name) && l.interner.is_value_type(f.ret)
}

/// `name__abi(args)`, the plain signature indirect callers use: a buffer-returning body gets the
/// heap box they expect, and a caller-tracking body gets a NULL location (`Fx::call` appends it).
pub(super) fn build_abi_wrapper<'a>(l: &mut Lcx<'a>, f: &'a MirFunction) {
    let name = l.user_fn(f);
    if !l.has_abi_wrapper(&name) {
        return;
    }
    let wrapper = l.abi_sym(&name);
    let n = l.sig(&wrapper).fty.params.len();
    let mut fx = super::glue::glue(l, &wrapper);
    let mut args: Vec<V> = (0..n).map(|i| fx.arg(i)).collect();
    if fx.l.sret.contains(&name) {
        let size = crate::backend::shared::abi_types::elem_size(&fx.l.cx, f.ret) as i64;
        let tag = fx.l.cx.type_tag(f.ret);
        let b = fx.call_v("dream_malloc", &[V::i64(size), V::i32(tag as i64)]);
        args.push(V::s(fx.ptr(&b)));
        fx.call(&name, &args);
        let r = fx.as_ref(&b);
        fx.w.ret(Some(&r.v));
    } else {
        let r = fx.call(&name, &args);
        fx.w.ret(r.as_ref().map(|r| &r.v));
    }
    fx.finish();
}

pub(super) fn poll_name(l: &Lcx<'_>, f: &MirFunction) -> String {
    format!("poll_{}", l.user_fn(f))
}

pub(super) fn drop_name(l: &Lcx<'_>, f: &MirFunction) -> String {
    format!("drop_{}", l.user_fn(f))
}

/// Frame offset of every local of the pre-lowered poll body.
pub(super) fn async_offsets(l: &Lcx<'_>, body: &MirFunction) -> (Vec<i32>, i32) {
    let cx = &l.cx;
    let fut = cx.target.abi().future;
    let slots = crate::async_emit::layout_async_slots(
        body,
        cx.interner,
        fut.slots as i32,
        |ty| {
            let sz = native_scalar_size(cx, ty).0.max(8);
            (sz, cx.interner.is_value_type(ty))
        },
        // Pack only primitive scalars: RC and value locals stay in the frame until
        // `AsyncComplete`, since packing them overwrites a still-owned pointer.
        if !cx.debug_syms {
            Some(Box::new(|ty| {
                !cx.interner.is_rc_tracked(ty) && !cx.interner.is_value_type(ty)
            }))
        } else {
            None
        },
    );
    let offs = (0..body.locals.len())
        .map(|i| slots.offsets.get(&i).copied().unwrap_or(0))
        .collect();
    (offs, slots.frame_size)
}

/// The constructor half of an async function: allocate the lazy future, copy the params in.
pub(super) fn build_async_stub<'a>(
    l: &mut Lcx<'a>,
    stub: &'a MirFunction,
    body: &MirFunction,
    offs: &[i32],
    frame_size: i32,
    poll_idx: i32,
    environment: Option<i32>,
) {
    let name = l.user_fn(stub);
    let mut w = l.writer(&name);
    w.attrs
        .extend(inline_attr(stub, l.cx.leak_checks && l.cx.debug_syms));
    let wide = crate::backend::shared::abi_types::ref_int_locals(&l.cx, body);
    let mut fx = Fx::new(l, stub, w);
    let first = fx.w.new_block("body");
    fx.w.br(first);
    fx.w.switch_to(first);
    let s = fx.call_v(
        "dream_new_future",
        &[
            V::i64(frame_size as i64),
            V::i32(poll_idx as i64),
            V::i32(0),
        ],
    );
    let metadata = V::s(Value::global(format!("info_{}", drop_name(fx.l, stub))));
    fx.call("dream_set_type", &[s.clone(), metadata]);
    if let Some(offset) = environment {
        // Polls are lazy: the caller's funcbox may die or another call may replace g0.
        let env = fx.read_global(crate::Global(0));
        fx.call("dream_retain", std::slice::from_ref(&env));
        let gate = fx.call_v(
            "dream_cycle_store_begin",
            &[s.clone(), env.clone(), V::i32(0)],
        );
        let at = fx.addr(&s, offset as i64);
        fx.store_ty(&fx.h(), &at, &env, 8);
        fx.call("dream_cycle_store_end", &[gate]);
    }
    for (pi, p) in body.params.iter().enumerate() {
        let off = offs[p.0 as usize] as i64;
        let ty = body.local_ty(*p);
        let arg = V {
            v: fx.w.param(pi),
            unsigned: is_unsigned(fx.interner, ty),
        };
        let at = fx.addr(&s, off);
        if fx.is_value(ty) {
            let sz = native_scalar_size(&fx.l.cx, ty).0 as i64;
            let src = fx.ptr(&arg);
            fx.memcpy(&at, &src, &Value::i64(sz));
        } else {
            let t = if wide[p.0 as usize] {
                fx.h()
            } else {
                super::types::ll_ty(fx.interner, ty, &fx.h(), &fx.word())
            };
            fx.store_ty(&t, &at, &arg, align_at(&t, off));
        }
    }
    fx.w.ret(Some(&s.v));
    let w = fx.w;
    l.define(w);
}

/// A trivially-complete poll + drop pair for an async function without a coroutine body.
pub(super) fn build_empty_poll_drop(l: &mut Lcx<'_>, stub: &MirFunction) {
    let mut poll = l.writer(&poll_name(l, stub));
    poll.ret(Some(&Value::i32(0)));
    l.define(poll);
    let mut drop = l.writer(&drop_name(l, stub));
    drop.ret(None);
    l.define(drop);
}

impl<'l, 'a> Fx<'l, 'a> {
    fn poll_locals(&mut self, offs: &[i32]) {
        let f = self.f;
        let s = V::u(self.w.param(0));
        self.self_ = Some(s.v.clone());
        self.slots = (0..f.locals.len()).map(|_| None).collect();
        for (i, decl) in f.locals.iter().enumerate() {
            if matches!(self.interner.kind(decl.ty), TyKind::Void) {
                continue;
            }
            let l = Local(i as u32);
            let slot = self.scalar_slot(l);
            let (p, t) = (slot.ptr.clone(), slot.ty.clone());
            self.slots[i] = Some(slot);
            let at = self.addr(&s, offs[i] as i64);
            let v = if self.is_value(decl.ty) {
                self.as_ref(&V::s(at))
            } else {
                let unsigned = is_unsigned(self.interner, decl.ty);
                self.load_ty(t.clone(), &at, align_at(&t, offs[i] as i64), unsigned)
            };
            self.store_ty(&t, &p, &v, 8);
        }
        self.poll_offsets = offs.to_vec();
    }

    /// At a resume block, moves the awaited child's settled value into the destination local.
    fn await_handoff(&mut self, d: Local) {
        let s = self.self_ref_v();
        let fut = self.l.cx.target.abi().future;
        let at = self.addr(&s, fut.awaiting as i64);
        let ch = self.load_ty(self.h(), &at, 8, true);
        self.store_ty(&self.h(), &at, &V::s(Value::zero(self.h())), 8);
        let dest_ty = self.f.local_ty(d);
        if self.is_value(dest_ty) {
            let sz = native_scalar_size(&self.l.cx, dest_ty).0 as i64;
            let ra = self.addr(&ch, fut.result as i64);
            let r = self.load_ty(self.h(), &ra, 8, true);
            let dst = self.read_local(d);
            let (dp, rp) = (self.ptr(&dst), self.ptr(&r));
            self.memcpy(&dp, &rp, &Value::i64(sz));
            self.value_refs(dest_ty, &dst, true);
            return;
        }
        let (t, off) = match self.interner.kind(dest_ty) {
            TyKind::Prim(PrimTy::Long | PrimTy::ULong) => (Ty::I64, fut.wide),
            TyKind::Prim(PrimTy::ISize | PrimTy::USize)
                if self.l.cx.mir.layouts.target.ptr_size == 8 =>
            {
                (Ty::I64, fut.wide)
            }
            TyKind::Prim(PrimTy::Float) => (Ty::F32, fut.wide),
            TyKind::Prim(PrimTy::Double) => (Ty::F64, fut.wide),
            _ => (
                if self.l.cx.target.spec().capabilities.linear_memory {
                    Ty::I32
                } else {
                    Ty::I64
                },
                fut.result,
            ),
        };
        let va = self.addr(&ch, off as i64);
        let unsigned = off == fut.result;
        let v = self.load_ty(t, &va, 8, unsigned);
        if self.is_rc(dest_ty) {
            self.call(retain_sym(&self.l.cx, dest_ty), std::slice::from_ref(&v));
        }
        self.write_local(d, &v);
    }

    fn self_ref_v(&self) -> V {
        V::u(
            self.self_
                .clone()
                .unwrap_or_else(|| crate::internal_error!("await outside a poll function")),
        )
    }

    /// Every poll reloads all locals from the frame, so a block only writes back what it modified.
    fn spill_dirty(&mut self, bi: usize, resume_dest: &[Option<u32>], offs: &[i32]) {
        let f = self.f;
        let mut dirty: Vec<u32> = Vec::new();
        if let Some(d) = resume_dest[bi] {
            let ty = f.locals[d as usize].ty;
            if !matches!(self.interner.kind(ty), TyKind::Void) && !self.is_value(ty) {
                dirty.push(d);
            }
        }
        for s in &f.blocks[bi].stmts {
            let id = match s {
                Statement::Assign(Place::Local(l), _) => l.0,
                Statement::Release(Operand::Copy(Place::Local(l))) => l.0,
                _ => continue,
            };
            let decl = &f.locals[id as usize];
            if !matches!(self.interner.kind(decl.ty), TyKind::Void)
                && !self.is_value(decl.ty)
                && !dirty.contains(&id)
            {
                dirty.push(id);
            }
        }
        let s = self.self_ref_v();
        for i in dirty {
            let v = self.read_local(Local(i));
            let t = self.local_ll(Local(i));
            let at = self.addr(&s, offs[i as usize] as i64);
            self.store_ty(&t, &at, &v, align_at(&t, offs[i as usize] as i64));
        }
    }
}

pub(super) fn build_poll<'a>(
    l: &mut Lcx<'a>,
    stub: &MirFunction,
    body: &'a MirFunction,
    offs: &[i32],
    environment: Option<i32>,
) {
    let name = poll_name(l, stub);
    let w = l.writer(&name);
    let mut fx = Fx::new(l, body, w);
    fx.debug_begin(stub);
    fx.source_begin(stub);
    fx.map_blocks();
    fx.poll_locals(offs);
    fx.poll_environment = environment;
    fx.debug_locals(false);
    let s = V::u(fx.w.param(0));
    let state_at = fx.addr(&s, fx.l.cx.target.abi().future.state as i64);
    let st = fx.load_ty(Ty::I32, &state_at, 4, false);
    let arms: Vec<(i128, BlockRef)> = (0..body.blocks.len())
        .filter(|bi| *bi != body.entry.0 as usize)
        .map(|bi| (bi as i128, fx.blocks[bi]))
        .collect();
    let entry = fx.blocks[body.entry.0 as usize];
    fx.w.switch(&st.v, entry, &arms);
    let mut resume_dest: Vec<Option<u32>> = vec![None; body.blocks.len()];
    for block in &body.blocks {
        if let Terminator::Await {
            dest: Some(d),
            resume,
            ..
        } = &block.terminator
            && (resume.0 as usize) < resume_dest.len()
        {
            resume_dest[resume.0 as usize] = Some(d.0);
        }
    }
    fx.body_blocks(&resume_dest, Some(offs));
    let w = fx.w;
    l.define(w);
}

fn drop_slot_rank(l: &Lcx<'_>, ty: dream_types::TypeId) -> u8 {
    match l.interner.kind(ty) {
        TyKind::Func(_, _) => 0,
        _ if l.interner.is_value_type(ty) => 2,
        _ if l.interner.is_rc_tracked(ty) => 1,
        _ => 3,
    }
}

/// Releases every owned reference still parked in an abandoned future's frame (the settled
/// result slot excepted: it is moved out by whoever awaits it).
pub(super) fn build_future_drop<'a>(
    l: &mut Lcx<'a>,
    stub: &MirFunction,
    body: &'a MirFunction,
    offs: &[i32],
    environment: Option<i32>,
) {
    let name = drop_name(l, stub);
    let mut idxs: Vec<usize> = (0..body.locals.len())
        .filter(|&i| {
            let d = &body.locals[i];
            if (!l.interner.is_rc_tracked(d.ty) && !l.interner.is_value_type(d.ty))
                || d.is_cursor
                || d.is_ref
                || crate::backend::shared::place_policy::is_alias_value_local(body, Local(i as u32))
            {
                return false;
            }
            let is_param = body.params.iter().any(|p| p.0 == i as u32);
            !is_param || d.is_take
        })
        .collect();
    let visit_name = format!("visit_{name}");
    let h = l.h();
    super::glue::register(l, &visit_name, Ty::Void, vec![h.clone()]);
    let mut visit = super::glue::glue(l, &visit_name);
    let owner = visit.arg(0);
    let result_at = visit.addr(&owner, visit.l.cx.target.abi().future.result as i64);
    let result = visit.load_ty(h.clone(), &result_at, 8, true);
    if let Some(offset) = environment {
        let at = visit.addr(&owner, offset as i64);
        let env = visit.load_ty(h.clone(), &at, 8, true);
        visit.call("dream_visit_edge", &[env]);
    }
    for &i in &idxs {
        let at = visit.addr(&owner, offs[i] as i64);
        let ty = body.locals[i].ty;
        if visit.is_value(ty) {
            let base = visit.ptr_value(&at);
            let ne = visit.w.icmp("ne", &base.v, &result.v);
            visit.if_then(&ne, |fx| fx.visit_refs(ty, &base));
        } else {
            let child = visit.load_ty(h.clone(), &at, align_at(&h, offs[i] as i64), true);
            visit.call("dream_visit_edge", std::slice::from_ref(&child));
        }
    }
    if visit.is_value(body.ret) {
        let nz = visit.truthy(&result);
        visit.if_then(&nz, |fx| fx.visit_refs(body.ret, &result));
    } else if visit.is_rc(body.ret) {
        visit.call("dream_visit_edge", std::slice::from_ref(&result));
    }
    visit.w.ret(None);
    visit.finish();
    super::glue::ownership::descriptor(
        l,
        &format!("info_{name}"),
        &visit_name,
        None,
        &name,
        true,
        false,
    );
    idxs.sort_by_key(|&i| drop_slot_rank(l, body.locals[i].ty));
    let w = l.writer(&name);
    let mut fx = Fx::new(l, body, w);
    let first = fx.w.new_block("body");
    fx.w.br(first);
    fx.w.switch_to(first);
    let s = V::u(fx.w.param(0));
    let ra = fx.addr(&s, fx.l.cx.target.abi().future.result as i64);
    let h = fx.h();
    let res = fx.load_ty(h.clone(), &ra, 8, true);
    if let Some(offset) = environment {
        let at = fx.addr(&s, offset as i64);
        let env = fx.load_ty(h.clone(), &at, 8, true);
        fx.store_ty(&h, &at, &V::s(Value::zero(h.clone())), 8);
        fx.call("dream_release_closure_env", &[env]);
    }
    for i in idxs {
        let ty = body.locals[i].ty;
        let at = fx.addr(&s, offs[i] as i64);
        if fx.is_value(ty) {
            let base = fx.ptr_value(&at);
            let ne = fx.w.icmp("ne", &base.v, &res.v);
            fx.if_then(&ne, |fx| fx.clear_refs(ty, &base));
            continue;
        }
        let v = fx.load_ty(h.clone(), &at, align_at(&h, offs[i] as i64), true);
        let nz = fx.truthy(&v);
        let both = nz;
        let rel = release_sym(&fx.l.cx, ty);
        fx.if_then(&both, |fx| {
            fx.store_ty(&h, &at, &V::s(Value::zero(h.clone())), 8);
            fx.call(&rel, std::slice::from_ref(&v));
        });
    }
    if fx.is_value(body.ret) {
        let nz = fx.truthy(&res);
        fx.if_then(&nz, |fx| fx.clear_refs(body.ret, &res));
    } else if fx.is_rc(body.ret) {
        let nz = fx.truthy(&res);
        let rel = release_sym(&fx.l.cx, body.ret);
        fx.if_then(&nz, |fx| {
            fx.store_ty(&h, &ra, &V::s(Value::zero(h.clone())), 8);
            fx.call(&rel, std::slice::from_ref(&res));
        });
    }
    fx.w.ret(None);
    let w = fx.w;
    l.define(w);
}

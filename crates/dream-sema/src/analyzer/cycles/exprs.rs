//! Expression transfer: what each expression may denote, and the stores and calls inside it.

use super::summary::Origin;
use super::value::{Atom, Atoms, PathId, Slot, frame_slot, one, result_slot, union};
use super::visit;
use super::walker::Walker;
use dream_hir::{Binding, GlobalId, HExpr, HExprKind, ParamMode};
use dream_types::{DefId, TyKind, TypeId};
use smallvec::SmallVec;

type Args = SmallVec<[Atoms; 4]>;

/// The synthetic `$__closure_env` slot, always registered first. An indirect call publishes the
/// callee's environment there and the callee's prologue reads it back.
pub(super) const CLOSURE_ENV: GlobalId = GlobalId(0);

impl Walker<'_, '_> {
    pub(super) fn eval(&mut self, e: &HExpr) -> Atoms {
        use HExprKind as K;
        let key = e as *const HExpr as usize;
        match &e.kind {
            K::Var(Binding::Local(local)) => self.state.local(local.0 as usize),
            K::Var(Binding::Global(global)) if *global == CLOSURE_ENV => self.env_param(),
            K::Var(Binding::Global(_)) => self.old_if(e.ty),
            K::Zero if self.bearing(e.ty) && self.prog.graph.interner().is_value_type(e.ty) => {
                let site = self.site(key, 0);
                one(self.typed(site, e.ty))
            }
            K::Call { callee, args } => {
                let atoms = self.eval_all(args.iter());
                let types: SmallVec<[TypeId; 4]> = args.iter().map(|a| a.ty).collect();
                let out = self.call(callee.def, &callee.instance, &atoms, &types, key, e.ty);
                self.rebind_refs(callee.def, &callee.instance, args.iter());
                out
            }
            K::MethodCall {
                receiver,
                callee,
                args,
            } => {
                let all = || std::iter::once(&**receiver).chain(args);
                let atoms = self.eval_all(all());
                let types: SmallVec<[TypeId; 4]> = all().map(|a| a.ty).collect();
                let out = self.call(callee.def, &callee.instance, &atoms, &types, key, e.ty);
                self.rebind_refs(callee.def, &callee.instance, all());
                out
            }
            K::IndirectCall { target, sig, args } => {
                self.eval(target);
                let mut atoms = self.eval_all(args.iter());
                atoms.push(self.closure_env.clone());
                let targets = Some(self.prog.indirect_targets(*sig).to_vec());
                let object = self.prog.graph.interner().object();
                let types = args.iter().map(|a| a.ty).chain(std::iter::once(object));
                self.dispatch(targets, &atoms, types, key, e.ty)
            }
            K::InterfaceCall {
                receiver,
                iface_id,
                method_slot,
                args,
                ..
            } => {
                let all = || std::iter::once(&**receiver).chain(args);
                let atoms = self.eval_all(all());
                let targets = self.prog.interface_targets(*iface_id, *method_slot);
                self.dispatch(targets, &atoms, all().map(|a| a.ty), key, e.ty)
            }
            K::New {
                instance,
                ctor,
                args,
                ..
            } => {
                let mut atoms = self.eval_all(args.iter());
                let this = if self.bearing(e.ty) {
                    let site = self.site_holding(key, 0, &mut atoms);
                    one(self.typed(site, e.ty))
                } else {
                    Atoms::new()
                };
                if let Some(ctor) = ctor {
                    let mut full: Args = std::iter::once(this.clone()).collect();
                    full.extend(atoms);
                    match self.prog.lookup(*ctor, instance) {
                        Some(index) => {
                            self.call_fn(index, &full, key, 1);
                        }
                        None => {
                            let types = std::iter::once(e.ty).chain(args.iter().map(|a| a.ty));
                            let typed: Vec<_> = full.into_iter().zip(types).collect();
                            self.havoc(&typed, e.ty, key);
                        }
                    }
                }
                this
            }
            K::UnionNew { args, .. } => {
                let atoms = self.eval_all(args.iter());
                let mut flat = Atoms::new();
                for a in &atoms {
                    union(&mut flat, a);
                }
                if !self.bearing(e.ty) {
                    Atoms::new()
                } else if self.prog.graph.interner().is_niche_union(e.ty) {
                    flat
                } else {
                    let site = self.site_holding(key, 0, std::slice::from_mut(&mut flat));
                    let site = self.typed(site, e.ty);
                    self.store(&[site], None, &flat, None);
                    one(site)
                }
            }
            K::ArrayLit { elems, .. } | K::Tuple { elems } => {
                let mut atoms = self.eval_all(elems.iter());
                if !self.bearing(e.ty) {
                    return Atoms::new();
                }
                let site = self.site_holding(key, 0, &mut atoms);
                let site = self.typed(site, e.ty);
                for a in &atoms {
                    self.store(&[site], None, a, None);
                }
                one(site)
            }
            K::ArrayNew { len, .. } => {
                self.eval(len);
                if self.bearing(e.ty) {
                    let site = self.site(key, 0);
                    one(self.typed(site, e.ty))
                } else {
                    Atoms::new()
                }
            }
            K::Field { obj, field } => {
                let base = self.eval(obj);
                if self.bearing(e.ty) {
                    let slot = self.field_slot(obj.ty, *field);
                    self.read(&base, slot)
                } else {
                    Atoms::new()
                }
            }
            K::Index { array, index } | K::ArrayGetUnchecked { array, index } => {
                let mut base = self.eval(array);
                self.eval_holding(&mut base, index);
                if self.bearing(e.ty) {
                    self.read(&base, Slot::Elem(array.ty))
                } else {
                    Atoms::new()
                }
            }
            K::ArraySlotTake { array, index, .. } => {
                let mut base = self.eval(array);
                self.eval_holding(&mut base, index);
                let value = if self.bearing(e.ty) {
                    self.read(&base, Slot::Elem(array.ty))
                } else {
                    Atoms::new()
                };
                self.wrote(Slot::Elem(array.ty), &base);
                value
            }
            K::UnionField {
                base,
                union_ty,
                variant,
                field,
            } => {
                let atoms = self.eval(base);
                if self.bearing(e.ty) {
                    self.payload(&atoms, *union_ty, *variant as u32, *field as u32)
                } else {
                    Atoms::new()
                }
            }
            K::ArrayRealloc { array, new_len, .. } => {
                let mut atoms = self.eval(array);
                self.eval_holding(&mut atoms, new_len);
                self.wrote(Slot::Elem(array.ty), &atoms);
                atoms
            }
            K::ArraySetUnchecked {
                array,
                index,
                value,
            } => {
                let operands = self.eval_all([&**array, &**index, &**value].into_iter());
                self.store_elem(&operands[0], array.ty, &operands[2], value.ty);
                Atoms::new()
            }
            K::ArrayElemsCopy {
                dst,
                dst_off,
                src,
                src_off,
                count,
                ..
            } => {
                let operands = self.eval_all(
                    [&**dst, &**dst_off, &**src, &**src_off, &**count].into_iter(),
                );
                let moved = self.read(&operands[2], Slot::Elem(src.ty));
                let elem = self.elem_of(src.ty);
                self.store_elem(&operands[0], dst.ty, &moved, elem);
                Atoms::new()
            }
            K::ArraySlotSwap {
                array,
                left,
                other_array,
                right,
                ..
            } => {
                let mut operands = self.eval_all(
                    [&**array, &**left, &**other_array, &**right].into_iter(),
                );
                let second = std::mem::take(&mut operands[2]);
                let first = std::mem::take(&mut operands[0]);
                if first == second && first.len() == 1 && self.single_object(first[0]) {
                    // Permuting one array's elements adds no reachability and every element
                    // stays beneath the array.
                    return Atoms::new();
                }
                let from_first = self.read(&first, Slot::Elem(array.ty));
                let from_second = self.read(&second, Slot::Elem(other_array.ty));
                let elem = self.elem_of(array.ty);
                self.store_elem(&second, other_array.ty, &from_first, elem);
                self.store_elem(&first, array.ty, &from_second, elem);
                Atoms::new()
            }
            K::ArrayElemsFill { dst, .. } => {
                let children = visit::children(e);
                let mut operands = self.eval_all(children.iter().copied());
                let targets = children
                    .iter()
                    .position(|child| std::ptr::eq(*child, &**dst))
                    .map(|at| std::mem::take(&mut operands[at]))
                    .unwrap_or_default();
                self.wrote(Slot::Elem(dst.ty), &targets);
                Atoms::new()
            }
            K::Cast(inner) => self.eval(inner),
            K::Ternary {
                cond,
                then_expr,
                else_expr,
            } => {
                self.eval(cond);
                let mut atoms = self.eval(then_expr);
                let other = self.eval_holding(&mut atoms, else_expr);
                union(&mut atoms, &other);
                atoms
            }
            K::Await(inner) => {
                let future = self.eval(inner);
                self.suspend();
                if !self.bearing(e.ty) {
                    return Atoms::new();
                }
                // A future holds its result directly, so an allocation site's own contents
                // bound what awaiting it yields: only the result when every object of the site
                // is an async call's future that kept it apart from its frame.
                let result = result_slot(inner.ty);
                let frame = frame_slot(inner.ty);
                let mut out = Atoms::new();
                for atom in future {
                    match atom {
                        Atom::Fresh(id) => {
                            let untracked = self.state.untracked_contents(id);
                            let Some(site) = self.state.site(id) else {
                                continue;
                            };
                            let apart = untracked.is_empty()
                                && site
                                    .contents
                                    .buckets()
                                    .all(|(slot, _)| slot == Some(result) || slot == Some(frame));
                            if apart {
                                site.contents.get(result, &mut out);
                            } else {
                                let held: Atoms = site.contents.atoms(true).collect();
                                union(&mut out, &held);
                                union(&mut out, untracked);
                            }
                        }
                        other => {
                            let result = self.extend(other, PathId::ANY);
                            union(&mut out, &result);
                        }
                    }
                }
                out
            }
            K::FromBytes(inner) | K::WireDecode(inner) | K::RawLoad(inner) => {
                self.eval(inner);
                self.old_if(e.ty)
            }
            K::JsCall { .. } => {
                for child in visit::children(e) {
                    let atoms = self.eval(child);
                    self.retain(&atoms);
                }
                self.old_if(e.ty)
            }
            _ => {
                for child in visit::children(e) {
                    self.eval(child);
                }
                Atoms::new()
            }
        }
    }

    /// A local passed as a `ref` argument may hold any object the callee assigned.
    fn rebind_refs<'e>(
        &mut self,
        def: DefId,
        instance: &[TypeId],
        args: impl Iterator<Item = &'e HExpr>,
    ) {
        let modes: Option<SmallVec<[ParamMode; 4]>> = self
            .prog
            .lookup(def, instance)
            .map(|index| self.prog.functions[index].params.iter().map(|p| p.mode).collect());
        let builtins = &self.prog.builtins;
        if modes.is_none() && (builtins.inspecting.contains(&def) || builtins.allocating.contains(&def)) {
            return;
        }
        for (i, arg) in args.enumerate() {
            let is_ref = modes
                .as_ref()
                .is_none_or(|modes| modes.get(i) == Some(&ParamMode::Ref));
            if is_ref
                && self.bearing(arg.ty)
                && let HExprKind::Var(Binding::Local(local)) = &arg.kind
            {
                let mut atoms = self.state.local(local.0 as usize);
                if super::value::insert(&mut atoms, Atom::Old) {
                    self.state.set_local(local.0 as usize, atoms);
                }
            }
        }
    }

    fn old_if(&self, ty: TypeId) -> Atoms {
        if self.bearing(ty) {
            one(Atom::Old)
        } else {
            Atoms::new()
        }
    }

    /// `atom` denotes exactly one object: an uncut access path, or a site's newest allocation.
    fn single_object(&self, atom: Atom) -> bool {
        match atom {
            Atom::Param(_, path) => path.is_exact(),
            Atom::Fresh(id) => id % 2 == 0,
            Atom::Old => false,
        }
    }

    fn elem_of(&self, array: TypeId) -> TypeId {
        match self.prog.graph.interner().kind(array) {
            TyKind::Array(elem) => *elem,
            _ => array,
        }
    }

    fn store_elem(&mut self, targets: &[Atom], array: TypeId, value: &[Atom], value_ty: TypeId) {
        let slot = Slot::Elem(array);
        let check = self.prog.graph.relevant(array, value_ty);
        let origin = Origin {
            container: array,
            slot,
            value: value_ty,
        };
        self.store(targets, Some(slot), value, check.then_some(origin));
        self.wrote(slot, targets);
    }

    pub(super) fn payload(
        &mut self,
        base: &[Atom],
        union_ty: TypeId,
        variant: u32,
        field: u32,
    ) -> Atoms {
        if self.prog.graph.interner().is_niche_union(union_ty) {
            base.iter().copied().collect()
        } else {
            self.read(base, Slot::Payload(union_ty, variant, field))
        }
    }

    fn call(
        &mut self,
        def: DefId,
        instance: &[TypeId],
        args: &[Atoms],
        types: &[TypeId],
        key: usize,
        ret: TypeId,
    ) -> Atoms {
        if let Some(index) = self.prog.lookup(def, instance) {
            return self.call_fn(index, args, key, 1);
        }
        let builtins = &self.prog.builtins;
        if Some(def) == builtins.funcbox_new {
            let mut env = args.get(1).cloned().unwrap_or_default();
            let site = self.site_holding(key, 0, std::slice::from_mut(&mut env));
            let site = self.typed(site, ret);
            self.store(&[site], None, &env, None);
            return one(site);
        }
        if Some(def) == builtins.funcbox_env {
            let boxed = args.first().cloned().unwrap_or_default();
            let ty = types.first().copied().unwrap_or(ret);
            return self.read(&boxed, Slot::Field(ty, 1));
        }
        if Some(def) == builtins.funcbox_funcidx {
            return Atoms::new();
        }
        if builtins.inspecting.contains(&def) {
            return self.old_if(ret);
        }
        if builtins.allocating.contains(&def) {
            return if self.bearing(ret) {
                let site = self.site(key, 0);
                one(self.typed(site, ret))
            } else {
                Atoms::new()
            };
        }
        // Host and runtime functions cannot link Dream objects, but may retain them.
        let mut retained = false;
        for atoms in args {
            retained |= !atoms.is_empty();
            self.retain(atoms);
        }
        if !builtins.intrinsics.contains(&def) {
            // A host function may run Dream callbacks this walk does not replay.
            self.suspend();
        } else if retained {
            self.wrote_all();
        }
        self.old_if(ret)
    }

    fn call_fn(&mut self, index: usize, args: &[Atoms], key: usize, part: u32) -> Atoms {
        let summaries = self.summaries;
        let refs: SmallVec<[bool; 4]> = self.prog.functions[index]
            .params
            .iter()
            .map(|p| p.mode == ParamMode::Ref)
            .collect();
        self.instantiate(&summaries[index], args, key, part, &refs)
    }

    fn dispatch(
        &mut self,
        targets: Option<Vec<usize>>,
        args: &[Atoms],
        types: impl Iterator<Item = TypeId>,
        key: usize,
        ret: TypeId,
    ) -> Atoms {
        match targets {
            Some(targets) => {
                let mut out = Atoms::new();
                for (n, index) in targets.into_iter().enumerate() {
                    let atoms = self.call_fn(index, args, key, n as u32 + 1);
                    union(&mut out, &atoms);
                }
                out
            }
            None => {
                let typed: Vec<_> = args.iter().cloned().zip(types).collect();
                self.havoc(&typed, ret, key)
            }
        }
    }
}

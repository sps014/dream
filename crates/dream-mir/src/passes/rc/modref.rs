//! Type-level mod-ref summaries: which heap slots a function may overwrite, closed over direct
//! calls and constructors.
//!
//! Field slots are keyed by `(base type, field index)`. Monomorphization makes every base type
//! concrete and Dream has no class inheritance, so a store through any local of type `T` can
//! only overwrite `(T, field)` — the key is exact. Array-element and global slots are keyed by
//! the RC type of the value they hold. Anything that can run code we cannot see (indirect /
//! interface / `js` calls, host imports, async entry, user `to_string` via the object protocol)
//! is [`ModRef::Top`].

use crate::{Callee, Global, Mir, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_abi::intrinsics::IntrinsicOp;
use dream_hir::LayoutTable;
use dream_types::{DefId, TyKind, TypeId, TypeInterner};
use indexmap::IndexMap;
use std::collections::BTreeSet;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Known {
    /// `(base type, field)` slots that may be overwritten.
    pub(crate) fields: BTreeSet<(TypeId, u32)>,
    /// RC value types of array-element / global slots that may be overwritten.
    pub(crate) slots: BTreeSet<TypeId>,
    /// Some array-element slot of an unknown (possibly RC) type may be overwritten.
    pub(crate) any_slot: bool,
    /// May read an object's refcount or the live-object count (`Debug.ref_count` & co.).
    pub(crate) observes_rc: bool,
}

/// What a region of code may do to the heap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ModRef {
    Known(Known),
    Top,
}

impl Default for ModRef {
    fn default() -> Self {
        ModRef::Known(Known::default())
    }
}

impl ModRef {
    fn known_mut(&mut self) -> Option<&mut Known> {
        match self {
            ModRef::Known(k) => Some(k),
            ModRef::Top => None,
        }
    }

    fn join(&mut self, other: &ModRef) -> bool {
        match (&mut *self, other) {
            (ModRef::Top, _) => false,
            (_, ModRef::Top) => {
                *self = ModRef::Top;
                true
            }
            (ModRef::Known(a), ModRef::Known(b)) => {
                let before = (a.fields.len(), a.slots.len(), a.any_slot, a.observes_rc);
                a.fields.extend(b.fields.iter().copied());
                a.slots.extend(b.slots.iter().copied());
                a.any_slot |= b.any_slot;
                a.observes_rc |= b.observes_rc;
                before != (a.fields.len(), a.slots.len(), a.any_slot, a.observes_rc)
            }
        }
    }

    /// True if a stored field slot is in `fields`, or is any field of a type in `any_field_of`.
    pub(crate) fn hits(&self, fields: &BTreeSet<(TypeId, u32)>, any_field_of: &BTreeSet<TypeId>) -> bool {
        match self {
            ModRef::Top => true,
            ModRef::Known(k) => k
                .fields
                .iter()
                .any(|key| fields.contains(key) || any_field_of.contains(&key.0)),
        }
    }

    pub(crate) fn observes_rc(&self) -> bool {
        match self {
            ModRef::Top => true,
            ModRef::Known(k) => k.observes_rc,
        }
    }

    /// No stores and no calls that store. Cannot run a `del`: there is nothing it releases
    /// except a borrowed parameter, and those are not freed by the callee.
    pub(crate) fn is_quiet(&self) -> bool {
        match self {
            ModRef::Top => false,
            ModRef::Known(k) => {
                k.fields.is_empty() && k.slots.is_empty() && !k.any_slot && !k.observes_rc
            }
        }
    }
}

/// Module-wide summaries. A lookup that misses (an empty table in unit tests, a pruned callee)
/// answers [`ModRef::Top`].
#[derive(Default)]
pub(crate) struct ModRefTable {
    fns: IndexMap<(DefId, Vec<TypeId>), ModRef>,
    /// Summary of `New { ctor }`: the constructor body minus its direct stores into the fresh
    /// `this` (no other reference to that object exists yet). Joined over instances.
    ctors: IndexMap<DefId, ModRef>,
    /// Bodiless intrinsic defs whose runtime helper overwrites no Dream heap slot.
    intrinsics: IndexMap<DefId, ModRef>,
    /// Union of every `del` body: what may run inside any `Release` that frees an object.
    del: ModRef,
    /// `(iface id, method slot)` → join of the concrete methods. Missing slots are [`ModRef::Top`].
    ifaces: IndexMap<(usize, usize), ModRef>,
    /// Layout names with a `<Name>_del`; `None` (an uncomputed table) means any type may.
    del_types: Option<BTreeSet<String>>,
}

impl ModRefTable {
    pub(crate) fn compute(mir: &Mir, interner: &TypeInterner) -> Self {
        let intrinsics: IndexMap<DefId, ModRef> = mir
            .intrinsics
            .iter()
            .filter_map(|(def, key)| intrinsic_summary(IntrinsicOp::from_key(key)?).map(|s| (*def, s)))
            .chain(
                mir.imports
                    .iter()
                    .filter(|i| pure_math_import(i, interner))
                    .map(|i| (i.def, ModRef::Known(Known::default()))),
            )
            .collect();
        let globals: IndexMap<Global, TypeId> = mir.globals.iter().map(|g| (g.id, g.ty)).collect();
        let locals: Vec<LocalSummary> = mir
            .functions
            .iter()
            .map(|f| LocalSummary::of(f, interner, &globals))
            .collect();
        let mut table = ModRefTable {
            fns: IndexMap::new(),
            ctors: IndexMap::new(),
            intrinsics,
            del: ModRef::default(),
            ifaces: IndexMap::new(),
            del_types: Some(
                mir.functions
                    .iter()
                    .filter_map(|f| f.name.strip_suffix("_del").map(str::to_string))
                    .collect(),
            ),
        };
        for (f, s) in mir.functions.iter().zip(&locals) {
            table.fns.insert((f.def, f.instance.clone()), s.own.clone());
            let entry = table.ctors.entry(f.def).or_default();
            entry.join(&s.own_fresh);
        }
        loop {
            let mut changed = false;
            for (f, s) in mir.functions.iter().zip(&locals) {
                let mut acc = s.own.clone();
                let mut acc_fresh = s.own_fresh.clone();
                for edge in &s.edges {
                    let callee = table.edge(edge);
                    acc.join(&callee);
                    acc_fresh.join(&callee);
                }
                let key = (f.def, f.instance.clone());
                changed |= table.fns.get_mut(&key).is_some_and(|cur| cur.join(&acc));
                changed |= table
                    .ctors
                    .get_mut(&f.def)
                    .is_some_and(|cur| cur.join(&acc_fresh));
            }
            if !changed {
                break;
            }
        }
        let mut del = ModRef::default();
        for f in &mir.functions {
            if f.name.ends_with("_del") {
                del.join(&table.call_def(f.def, &f.instance));
            }
        }
        table.del = del;
        table.close_ifaces(mir, &locals);
        table
    }

    /// Interface calls are edges to a closed set of methods. The direct-call fixpoint above treats
    /// those edges as a no-op so a slot summary can be built from the methods, then folded back
    /// into callers. An implementor that itself dispatches is re-joined until the slot stops moving.
    fn close_ifaces(&mut self, mir: &Mir, locals: &[LocalSummary]) {
        let by_name: IndexMap<&str, &MirFunction> =
            mir.functions.iter().map(|f| (f.name.as_str(), f)).collect();
        loop {
            let mut iface: IndexMap<(usize, usize), ModRef> = IndexMap::new();
            for imp in &mir.interfaces.impls {
                for (iface_id, slots) in &imp.entries {
                    for (slot, sym) in slots.iter().enumerate() {
                        let summary = match by_name.get(sym.as_str()) {
                            Some(f) => self.call_def(f.def, &f.instance),
                            None => ModRef::Top,
                        };
                        iface.entry((*iface_id, slot)).or_default().join(&summary);
                    }
                }
            }
            for s in locals {
                for edge in &s.edges {
                    if let Edge::Iface(id, slot) = edge {
                        iface.entry((*id, *slot)).or_insert(ModRef::Top);
                    }
                }
            }
            let iface_changed = iface != self.ifaces;
            self.ifaces = iface;
            let mut grew = false;
            for (f, s) in mir.functions.iter().zip(locals) {
                let mut extra = ModRef::default();
                for edge in &s.edges {
                    if let Edge::Iface(id, slot) = edge {
                        extra.join(&self.iface(*id, *slot));
                    }
                }
                let key = (f.def, f.instance.clone());
                if let Some(cur) = self.fns.get_mut(&key) {
                    grew |= cur.join(&extra);
                }
            }
            if !grew && !iface_changed {
                break;
            }
            if !grew {
                break;
            }
        }
    }

    fn edge(&self, edge: &Edge) -> ModRef {
        match edge {
            Edge::Call(def, args) => self.call_def(*def, args),
            // Filled in by [`Self::close_ifaces`] after the direct-call fixpoint.
            Edge::Iface(_, _) => ModRef::default(),
            Edge::Ctor(def) => self.ctor(*def),
        }
    }

    pub(crate) fn call_def(&self, def: DefId, args: &[TypeId]) -> ModRef {
        if let Some(s) = self.fns.get(&(def, args.to_vec())) {
            return s.clone();
        }
        self.intrinsics.get(&def).cloned().unwrap_or(ModRef::Top)
    }

    pub(crate) fn call(&self, callee: &Callee) -> ModRef {
        self.call_def(callee.def, &callee.args)
    }

    pub(crate) fn ctor(&self, def: DefId) -> ModRef {
        self.ctors.get(&def).cloned().unwrap_or(ModRef::Top)
    }

    pub(crate) fn iface(&self, iface_id: usize, slot: usize) -> ModRef {
        self.ifaces
            .get(&(iface_id, slot))
            .cloned()
            .unwrap_or(ModRef::Top)
    }

    /// What any `del` may do; a `Release` anywhere may run one.
    pub(crate) fn del(&self) -> &ModRef {
        &self.del
    }

    /// Whether freeing a `ty` graph may run a `del` (walking strong fields; opaque types may
    /// hold anything).
    pub(crate) fn may_run_del(&self, ty: TypeId, interner: &TypeInterner, layouts: &LayoutTable) -> bool {
        let Some(dels) = &self.del_types else {
            return true;
        };
        if dels.is_empty() {
            return false;
        }
        let mut seen = BTreeSet::new();
        let mut stack = vec![ty];
        while let Some(t) = stack.pop() {
            if !seen.insert(t) {
                continue;
            }
            let name = match layouts.get(t) {
                Some(l) => Some(l.name.as_str()),
                None => layouts.union(t).map(|u| u.name.as_str()),
            };
            if name.is_some_and(|n| dels.contains(n)) {
                return true;
            }
            match strong_children(t, interner, layouts) {
                Some(cs) => stack.extend(cs),
                None => return true,
            }
        }
        false
    }
}

/// Types directly owned by a `ty` value through strong slots, or `None` if its contents are
/// unknown (`object`, interfaces, `fun`, `js`, missing layouts).
pub(crate) fn strong_children(
    ty: TypeId,
    interner: &TypeInterner,
    layouts: &LayoutTable,
) -> Option<Vec<TypeId>> {
    let strong = |fs: &[dream_hir::FieldLayout]| -> Vec<TypeId> {
        fs.iter()
            .filter(|f| !f.is_weak && !f.is_unowned)
            .map(|f| f.ty)
            .collect()
    };
    Some(match interner.kind(ty) {
        TyKind::Array(e) => vec![*e],
        TyKind::Tuple(ts) => ts.clone(),
        TyKind::Struct(..) => strong(&layouts.get(ty)?.fields),
        TyKind::Union(..) => layouts
            .union(ty)?
            .variants
            .iter()
            .flat_map(|v| strong(&v.fields))
            .collect(),
        TyKind::Prim(_) | TyKind::Void | TyKind::Error | TyKind::Enum(_) => Vec::new(),
        TyKind::Object | TyKind::Interface(..) | TyKind::Func(..) | TyKind::Js => return None,
    })
}

enum Edge {
    Call(DefId, Vec<TypeId>),
    Iface(usize, usize),
    Ctor(DefId),
}

struct LocalSummary {
    own: ModRef,
    own_fresh: ModRef,
    edges: Vec<Edge>,
}

impl LocalSummary {
    fn of(f: &MirFunction, interner: &TypeInterner, globals: &IndexMap<Global, TypeId>) -> Self {
        let this = f.params.first().copied();
        let mut s = LocalSummary {
            own: ModRef::default(),
            own_fresh: ModRef::default(),
            edges: Vec::new(),
        };
        if f.is_async {
            s.own = ModRef::Top;
            s.own_fresh = ModRef::Top;
            return s;
        }
        let record = |s: &mut LocalSummary, e: Effect<'_>| match e {
            Effect::None => {}
            Effect::Top => {
                s.own = ModRef::Top;
                s.own_fresh = ModRef::Top;
            }
            Effect::Store(base, field) => {
                let key = (f.local_ty(base), field);
                if let Some(k) = s.own.known_mut() {
                    k.fields.insert(key);
                }
                if Some(base) != this {
                    if let Some(k) = s.own_fresh.known_mut() {
                        k.fields.insert(key);
                    }
                }
            }
            Effect::SlotStore(ty) => {
                for m in [&mut s.own, &mut s.own_fresh] {
                    if let Some(k) = m.known_mut() {
                        k.slots.insert(ty);
                    }
                }
            }
            Effect::GlobalStore(g) => match globals.get(&g) {
                Some(&ty) if !interner.is_rc_tracked(ty) => {}
                Some(&ty) => {
                    for m in [&mut s.own, &mut s.own_fresh] {
                        if let Some(k) = m.known_mut() {
                            k.slots.insert(ty);
                        }
                    }
                }
                None => {
                    s.own = ModRef::Top;
                    s.own_fresh = ModRef::Top;
                }
            },
            Effect::Call(callee) => s.edges.push(Edge::Call(callee.def, callee.args.clone())),
            Effect::Iface(id, slot) => s.edges.push(Edge::Iface(id, slot)),
            Effect::Ctor(def) => s.edges.push(Edge::Ctor(def)),
        };
        for block in &f.blocks {
            for stmt in &block.stmts {
                let mut effects = Vec::new();
                stmt_effects(stmt, f, interner, |e| effects.push(e));
                for e in effects {
                    record(&mut s, e);
                }
            }
            match &block.terminator {
                Terminator::TailCall { callee, .. } => record(&mut s, Effect::Call(callee)),
                Terminator::Await { .. } => record(&mut s, Effect::Top),
                _ => {}
            }
        }
        s
    }
}

/// What one statement contributes to a mod-ref summary.
pub(crate) enum Effect<'a> {
    None,
    Top,
    /// Field store through `base`.
    Store(crate::Local, u32),
    /// Overwrite of an array-element slot holding this RC type.
    SlotStore(TypeId),
    GlobalStore(Global),
    Call(&'a Callee),
    /// Interface slot. The summary is the join of every implementor's method, not [`Effect::Top`],
    /// when that set is closed.
    Iface(usize, usize),
    Ctor(DefId),
}

/// Reports every [`Effect`] of `stmt` to `out`: an assignment can both run a call (the RHS) and
/// store into a slot (the place).
pub(crate) fn stmt_effects<'a>(
    stmt: &'a Statement,
    f: &MirFunction,
    interner: &TypeInterner,
    mut out: impl FnMut(Effect<'a>),
) {
    match stmt {
        Statement::Assign(place, rv) => {
            out(rvalue_effect(rv, f, interner));
            match place {
                Place::Deref { .. } => out(Effect::Top),
                Place::Field { base, field } => out(Effect::Store(*base, *field as u32)),
                Place::Index { base, .. } => match interner.kind(f.local_ty(*base)) {
                    TyKind::Array(e) if interner.is_rc_tracked(*e) => out(Effect::SlotStore(*e)),
                    TyKind::Array(_) => {}
                    _ => out(Effect::Top),
                },
                Place::Global(g) => out(Effect::GlobalStore(*g)),
                Place::Local(_) => {}
            }
        }
        Statement::Call { callee, .. } => out(Effect::Call(callee)),
        Statement::InterfaceCall {
            iface_id,
            method_slot,
            ..
        } => out(Effect::Iface(*iface_id, *method_slot)),
        Statement::JsCall { .. } | Statement::IndirectCall { .. } | Statement::ForceFree(_) => {
            out(Effect::Top)
        }
        Statement::Print { ty, .. } if !protocol_is_builtin(*ty, interner) => out(Effect::Top),
        Statement::ArrayElemsCopy { elem_ty, .. } | Statement::ArrayElemsFill { elem_ty, .. }
            if interner.is_rc_tracked(*elem_ty) =>
        {
            out(Effect::SlotStore(*elem_ty))
        }
        _ => {}
    }
}

fn rvalue_effect<'a>(rv: &'a Rvalue, f: &MirFunction, interner: &TypeInterner) -> Effect<'a> {
    match rv {
        Rvalue::Call { callee, .. } => Effect::Call(callee),
        Rvalue::New {
            ctor: Some(ctor), ..
        } => Effect::Ctor(ctor.def),
        Rvalue::InterfaceCall {
            iface_id,
            method_slot,
            ..
        } => Effect::Iface(*iface_id, *method_slot),
        Rvalue::IndirectCall { .. } | Rvalue::JsCall { .. } => Effect::Top,
        Rvalue::ArrayRealloc { elem_ty, .. } if interner.is_rc_tracked(*elem_ty) => {
            Effect::SlotStore(*elem_ty)
        }
        Rvalue::ToString(op) | Rvalue::HashCode(op) => {
            let ty = match op {
                Operand::Copy(Place::Local(l)) => Some(f.local_ty(*l)),
                Operand::Const(_) => None,
                _ => return Effect::Top,
            };
            match ty {
                Some(ty) if !protocol_is_builtin(ty, interner) => Effect::Top,
                _ => Effect::None,
            }
        }
        _ => Effect::None,
    }
}

/// `to_string` / `hash_code` / print of this type never reaches user code.
fn protocol_is_builtin(ty: TypeId, interner: &TypeInterner) -> bool {
    ty == interner.string() || matches!(interner.kind(ty), TyKind::Prim(_))
}

fn pure_math_import(imp: &dream_hir::HImport, interner: &TypeInterner) -> bool {
    let field = if imp.field.is_empty() { &imp.name } else { &imp.field };
    let numeric = |t: TypeId| {
        matches!(interner.kind(t), TyKind::Prim(p) if *p != dream_types::PrimTy::String)
    };
    dream_abi::runtime_hosts::is_pure_math_env_import(&imp.module, field)
        && !imp.is_async
        && !imp.param_by_ref.iter().any(|&r| r)
        && imp.params.iter().all(|&t| numeric(t))
        && imp.ret.is_none_or(numeric)
}

/// Summary of a bodiless intrinsic's runtime helper, or `None` (opaque) if it may run user code
/// or overwrite object slots.
fn intrinsic_summary(op: IntrinsicOp) -> Option<ModRef> {
    use IntrinsicOp as I;
    let observes = matches!(
        op,
        I::DebugFreeList
            | I::DebugHeapPtr
            | I::DebugLiveObjects
            | I::DebugTotalAllocations
            | I::DebugRefCount
    );
    let quiet = matches!(
        op,
        I::ArrayNew
            | I::StringAlloc
            | I::StringSet
            | I::StringFromUtf8
            | I::StringFromUtf8Prefix
            | I::StringFromUtf8PrefixN
            | I::StringSubstring
            | I::StringCopyUtf8
            | I::StringCompare
            | I::ToBytes
            | I::FromBytes
            | I::ArrayGetUnchecked
            | I::Regex
    ) || op.is_simd();
    // Element stores of these may release an RC occupant; the element type is not known here.
    let elem_store = matches!(
        op,
        I::ArrayRealloc | I::ArrayElemsCopy | I::ArrayElemsFill | I::ArraySetUnchecked
    );
    if !(observes || quiet || elem_store) {
        return None;
    }
    Some(ModRef::Known(Known {
        observes_rc: observes,
        any_slot: elem_store,
        ..Known::default()
    }))
}

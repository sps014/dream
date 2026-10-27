//! Local statements contribute stores, call edges and ownership obligations.

use super::{Known, ModRef};
use crate::analysis::ownership::OwnershipFacts;
use crate::{Callee, Global, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::{DefId, TyKind, TypeId, TypeInterner};
use indexmap::IndexMap;
use std::borrow::Cow;
use std::collections::BTreeSet;

pub(super) enum Edge {
    Call(DefId, Vec<TypeId>, Vec<Option<BTreeSet<usize>>>),
    Iface(usize, usize, Vec<Option<BTreeSet<usize>>>),
    Ctor(DefId),
}

pub(super) struct LocalSummary {
    pub(super) observes_rc: bool,
    pub(super) may_destroy: bool,
    pub(super) own: ModRef,
    pub(super) own_fresh: ModRef,
    pub(super) edges: Vec<Edge>,
}

impl LocalSummary {
    pub(super) fn of(
        f: &MirFunction,
        interner: &TypeInterner,
        globals: &IndexMap<Global, TypeId>,
        may_destroy: bool,
        precise: bool,
        rc_event_counters: &BTreeSet<DefId>,
    ) -> Self {
        let this = f.params.first().copied();
        let mut s = LocalSummary {
            observes_rc: false,
            may_destroy,
            own: ModRef::Known(Known {
                may_destroy,
                ..Known::default()
            }),
            own_fresh: ModRef::Known(Known {
                may_destroy,
                ..Known::default()
            }),
            edges: Vec::new(),
        };
        if f.is_async {
            s.observes_rc = true;
            s.may_destroy = true;
            s.own = ModRef::Top;
            s.own_fresh = ModRef::Top;
            return s;
        }
        let facts = if precise {
            OwnershipFacts::compute(f, interner)
        } else {
            OwnershipFacts::unknown()
        };
        let record = |s: &mut LocalSummary,
                      e: Effect<'_>,
                      bi: usize,
                      si: usize,
                      arguments: &[Operand]| match e {
            Effect::None => {}
            Effect::ObserveOwnership => {
                s.observes_rc = true;
                for effect in [&mut s.own, &mut s.own_fresh] {
                    if let Some(known) = effect.known_mut() {
                        known.observes_rc = true;
                    }
                }
            }
            Effect::Top => {
                s.own = ModRef::Top;
                s.own_fresh = ModRef::Top;
            }
            Effect::Store(base, field) => {
                let key = (f.local_ty(base), field);
                if let Some(k) = s.own.known_mut() {
                    k.record_field(key, facts.parameter_origins(bi, si, base));
                }
                if Some(base) != this
                    && let Some(k) = s.own_fresh.known_mut()
                {
                    k.record_field(key, facts.parameter_origins(bi, si, base));
                }
            }
            Effect::SlotStore(ty) => {
                let writes = f.blocks[bi]
                    .stmts
                    .get(si)
                    .and_then(|stmt| super::element_writes(stmt, f, interner))
                    .and_then(|writes| {
                        let mut roots = BTreeSet::new();
                        for write in writes.into_iter().filter(|write| write.element == ty) {
                            roots.extend(
                                facts
                                    .parameter_origins(bi, si, write.base)?
                                    .into_iter()
                                    .map(|root| (root, write.index)),
                            );
                        }
                        Some(roots)
                    });
                for m in [&mut s.own, &mut s.own_fresh] {
                    if let Some(k) = m.known_mut() {
                        k.record_slot(ty, writes.clone());
                    }
                }
            }
            Effect::GlobalStore(g) => match globals.get(&g) {
                Some(&ty) if !interner.is_rc_tracked(ty) => {}
                Some(&ty) => {
                    for m in [&mut s.own, &mut s.own_fresh] {
                        if let Some(k) = m.known_mut() {
                            k.record_slot(ty, None);
                        }
                    }
                }
                None => {
                    s.own = ModRef::Top;
                    s.own_fresh = ModRef::Top;
                }
            },
            Effect::Call(callee)
                if precise
                    && super::ModRefTable::is_rc_event_counter(
                        rc_event_counters,
                        callee.def,
                        arguments,
                    ) => {}
            Effect::Call(callee) => s.edges.push(Edge::Call(
                callee.def,
                callee.args.clone(),
                arguments
                    .iter()
                    .map(|op| match op {
                        Operand::Copy(Place::Local(l)) => facts.parameter_origins(bi, si, *l),
                        _ => None,
                    })
                    .collect(),
            )),
            Effect::Iface(id, slot) => s.edges.push(Edge::Iface(
                id,
                slot,
                arguments
                    .iter()
                    .map(|op| match op {
                        Operand::Copy(Place::Local(l)) => facts.parameter_origins(bi, si, *l),
                        _ => None,
                    })
                    .collect(),
            )),
            Effect::Ctor(def) => s.edges.push(Edge::Ctor(def)),
        };
        for (bi, block) in f.blocks.iter().enumerate() {
            for (si, stmt) in block.stmts.iter().enumerate() {
                // A store through a raw address has unknown writes, but does not itself
                // inspect ownership. Its RHS effects and possible cleanup are separate.
                let raw_write = matches!(stmt, Statement::RawStore { .. })
                    || matches!(stmt, Statement::Assign(Place::Deref { .. }, rv)
                        if !matches!(rv, Rvalue::RawLoad { .. } | Rvalue::IndirectCall { .. }
                            | Rvalue::JsCall { .. } | Rvalue::ToString(_) | Rvalue::HashCode(_)));
                let arguments = call_arguments(stmt);
                let mut effects = Vec::new();
                if matches!(stmt, Statement::DebugLine(_)) {
                    effects.push(Effect::ObserveOwnership);
                }
                stmt_effects(stmt, f, interner, |e| effects.push(e));
                for e in effects {
                    if matches!(e, Effect::Top) && !raw_write {
                        s.observes_rc = true;
                    }
                    record(&mut s, e, bi, si, &arguments);
                }
            }
            match &block.terminator {
                Terminator::TailCall { callee, args } => {
                    record(&mut s, Effect::Call(callee), bi, block.stmts.len(), args)
                }
                Terminator::Await { .. } => {
                    s.observes_rc = true;
                    s.may_destroy = true;
                    record(&mut s, Effect::Top, bi, block.stmts.len(), &[]);
                }
                _ => {}
            }
        }
        s
    }
}

/// Method summaries use receiver index zero, matching their concrete MIR bodies.
pub(crate) fn call_arguments(stmt: &Statement) -> Cow<'_, [Operand]> {
    match stmt {
        Statement::Call { args, .. } | Statement::Assign(_, Rvalue::Call { args, .. }) => {
            Cow::Borrowed(args)
        }
        Statement::InterfaceCall { receiver, args, .. }
        | Statement::Assign(_, Rvalue::InterfaceCall { receiver, args, .. }) => Cow::Owned(
            std::iter::once(receiver.clone())
                .chain(args.iter().cloned())
                .collect(),
        ),
        _ => Cow::Borrowed(&[]),
    }
}

/// What one statement contributes to a mod-ref summary.
pub(crate) enum Effect<'a> {
    None,
    Top,
    ObserveOwnership,
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
        Statement::RawStore { .. } => out(Effect::Top),
        Statement::Assign(place, rv) => {
            out(rvalue_effect(rv, f, interner));
            match place {
                Place::Deref { .. } => out(Effect::Top),
                Place::Field { base, field } => out(Effect::Store(*base, *field as u32)),
                Place::Index { base, .. } => match interner.kind(f.local_ty(*base)) {
                    TyKind::Array(e)
                        if interner.is_rc_tracked(*e) || interner.is_value_type(*e) =>
                    {
                        out(Effect::SlotStore(*e))
                    }
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
        Statement::ArrayElemsCopy { elem_ty, .. }
        | Statement::ArrayElemsFill { elem_ty, .. }
        | Statement::ArraySlotSwap { elem_ty, .. }
            if interner.is_rc_tracked(*elem_ty) || interner.is_value_type(*elem_ty) =>
        {
            out(Effect::SlotStore(*elem_ty))
        }
        _ => {}
    }
}

fn rvalue_effect<'a>(rv: &'a Rvalue, f: &MirFunction, interner: &TypeInterner) -> Effect<'a> {
    match rv {
        Rvalue::RawLoad { .. } => Effect::Top,
        Rvalue::ObservedLoad(_) => Effect::ObserveOwnership,
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
        Rvalue::ArraySlotTake { elem_ty, .. } | Rvalue::ArrayRealloc { elem_ty, .. }
            if interner.is_rc_tracked(*elem_ty) || interner.is_value_type(*elem_ty) =>
        {
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

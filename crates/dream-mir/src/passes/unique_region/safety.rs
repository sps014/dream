use super::*;

pub(super) struct SafeCx<'a> {
    pub(super) mir: &'a Mir,
    pub(super) interner: &'a TypeInterner,
    pub(super) ctor_only: &'a IndexSet<DefId>,
    pub(super) memo: &'a IndexMap<(DefId, Vec<TypeId>), bool>,
    pub(super) index: &'a FunctionIndex,
}

pub(super) struct FunctionIndex {
    instances: IndexMap<(DefId, Vec<TypeId>), usize>,
    definitions: IndexMap<DefId, Vec<usize>>,
}

impl FunctionIndex {
    pub(super) fn new(mir: &Mir) -> Self {
        let mut index = Self {
            instances: IndexMap::new(),
            definitions: IndexMap::new(),
        };
        for (i, f) in mir.functions.iter().enumerate() {
            index.instances.insert((f.def, f.instance.clone()), i);
            index.definitions.entry(f.def).or_default().push(i);
        }
        index
    }

    pub(super) fn find<'a>(
        &self,
        mir: &'a Mir,
        def: DefId,
        args: &[TypeId],
    ) -> Option<&'a MirFunction> {
        self.instances
            .get(&(def, args.to_vec()))
            .map(|&i| &mir.functions[i])
    }
}

pub(super) fn region_safe(cx: &mut SafeCx<'_>, f: &MirFunction) -> bool {
    let key = (f.def, f.instance.clone());
    cx.memo.get(&key).copied().unwrap_or(false)
}

pub(super) fn compute_safety(
    mir: &Mir,
    interner: &TypeInterner,
    ctor_only: &IndexSet<DefId>,
    index: &FunctionIndex,
) -> IndexMap<(DefId, Vec<TypeId>), bool> {
    let mut adjacency = vec![Vec::new(); mir.functions.len()];
    for (i, f) in mir.functions.iter().enumerate() {
        walk_fn(f, |statement| {
            let key = match statement {
                Statement::Call { callee, .. }
                | Statement::Assign(_, Rvalue::Call { callee, .. }) => {
                    Some((callee.def, callee.args.clone()))
                }
                Statement::Assign(
                    _,
                    Rvalue::New {
                        ctor: Some(ctor), ..
                    },
                ) => {
                    if let Some(targets) = index.definitions.get(&ctor.def) {
                        adjacency[i].extend(targets);
                    }
                    None
                }
                _ => None,
            };
            if let Some(key) = key
                && let Some(&target) = index.instances.get(&key)
            {
                adjacency[i].push(target);
            }
        });
    }
    let mut memo = IndexMap::new();
    // Tarjan emits dependency SCCs before their callers. Provisional truths stay inside
    // the current SCC until every recursive dependency has converged to the same verdict.
    for component in crate::passes::inline::graph::tarjan_scc(&adjacency) {
        for &i in &component {
            let f = &mir.functions[i];
            memo.insert((f.def, f.instance.clone()), true);
        }
        loop {
            let mut changed = false;
            for &i in &component {
                let f = &mir.functions[i];
                let key = (f.def, f.instance.clone());
                if !memo[&key] {
                    continue;
                }
                let mut cx = SafeCx {
                    mir,
                    interner,
                    ctor_only,
                    memo: &memo,
                    index,
                };
                if !region_safe_body(&mut cx, f) {
                    memo.insert(key, false);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }
    memo
}

pub(super) fn region_safe_body(cx: &mut SafeCx<'_>, f: &MirFunction) -> bool {
    if f.is_async {
        return false;
    }
    if cx.mir.intrinsics.iter().any(|(d, _)| *d == f.def) {
        return false;
    }
    if !private_graph_ok(cx, f) {
        return false;
    }
    let this_local = f.params.first().copied();
    let mut new_locals = IndexSet::new();
    for b in &f.blocks {
        for s in &b.stmts {
            if let Statement::Assign(Place::Local(d), Rvalue::New { .. }) = s {
                new_locals.insert(*d);
            }
        }
        match &b.terminator {
            Terminator::TailCall { .. } | Terminator::Await { .. } => return false,
            _ => {}
        }
    }
    for b in &f.blocks {
        for s in &b.stmts {
            if !stmt_region_safe(cx, f, s, this_local, &new_locals) {
                return false;
            }
        }
    }
    // The birth site adopts this function's result as region memory, so it is only sound if the
    // function allocated it. A getter hands back an occupant the container still owns — and for a
    // niche union (`JsonValue.get` returning `Option.Some(child)`) the wrapper *is* that child's
    // pointer, so the leave's alias nulls would drop the caller's reference to a live object.
    if cx.interner.is_rc_tracked(f.ret) && !returns_fresh(cx.interner, f) {
        return false;
    }
    true
}

/// Every `Return` hands back memory this function allocated, directly or through copies of such a
/// local, or null (a niche `None`: nothing for the caller's region to reclaim). A niche `UnionNew`
/// is excluded: it yields its payload's pointer rather than a new block.
///
/// Must-analysis: a local is fresh only if *every* definition is an allocation, null, or a copy of
/// a fresh local, so a local that is sometimes a borrowed occupant never qualifies. Parameters are
/// never fresh.
pub(super) fn returns_fresh(interner: &TypeInterner, f: &MirFunction) -> bool {
    let mut candidate: BTreeSet<u32> = BTreeSet::new();
    let mut copies: Vec<(u32, u32)> = Vec::new();
    let mut rejected: BTreeSet<u32> = f.params.iter().map(|p| p.0).collect();
    for b in &f.blocks {
        if let Terminator::Await { dest: Some(d), .. } = &b.terminator {
            rejected.insert(d.0);
        }
        for s in &b.stmts {
            let Statement::Assign(Place::Local(d), rv) = s else {
                continue;
            };
            candidate.insert(d.0);
            match rv {
                Rvalue::UnionNew { ty, .. } if interner.is_niche_union(*ty) => {
                    rejected.insert(d.0);
                }
                Rvalue::New { .. }
                | Rvalue::UnionNew { .. }
                | Rvalue::ArrayNew { .. }
                | Rvalue::ArrayLit { .. }
                | Rvalue::Tuple { .. }
                | Rvalue::Concat(_)
                | Rvalue::ConcatInt { .. }
                | Rvalue::ToString(_)
                | Rvalue::ToBytes { .. }
                | Rvalue::Use(Operand::Const(Const::Null)) => {}
                Rvalue::Use(Operand::Copy(Place::Local(s)))
                | Rvalue::Cast(Operand::Copy(Place::Local(s)), _, _) => copies.push((d.0, s.0)),
                _ => {
                    rejected.insert(d.0);
                }
            }
        }
    }
    let mut fresh: BTreeSet<u32> = candidate.difference(&rejected).copied().collect();
    let mut changed = true;
    while changed {
        changed = false;
        for (d, src) in &copies {
            if fresh.contains(d) && !fresh.contains(src) {
                fresh.remove(d);
                changed = true;
            }
        }
    }
    f.blocks.iter().all(|b| match &b.terminator {
        Terminator::Return(Some(Operand::Copy(Place::Local(l)))) => fresh.contains(&l.0),
        Terminator::Return(Some(Operand::Const(Const::Null))) => true,
        Terminator::Return(Some(_)) => false,
        _ => true,
    })
}

pub(super) fn callee_safe(cx: &mut SafeCx<'_>, callee: &Callee) -> bool {
    if cx.mir.intrinsics.iter().any(|(d, _)| *d == callee.def) {
        return false;
    }
    let Some(g) = cx.index.find(cx.mir, callee.def, &callee.args) else {
        return false;
    };
    region_safe(cx, g)
}

pub(super) fn stmt_region_safe(
    cx: &mut SafeCx<'_>,
    f: &MirFunction,
    stmt: &Statement,
    this_local: Option<Local>,
    new_locals: &IndexSet<Local>,
) -> bool {
    match stmt {
        Statement::Nop
        | Statement::DebugLine(_)
        | Statement::SourceLine(_)
        | Statement::Retain(_)
        | Statement::Release(_)
        | Statement::RegionEnter
        | Statement::RegionLeave => true,
        Statement::Print { .. }
        | Statement::Panic(_)
        | Statement::JsCall { .. }
        | Statement::InterfaceCall { .. }
        | Statement::IndirectCall { .. }
        | Statement::ForceFree(_)
        | Statement::LockAcquire(_)
        | Statement::LockRelease(_)
        | Statement::DeferEnter
        | Statement::DeferLeave(_)
        | Statement::Call { .. }
        | Statement::ArrayElemsCopy { .. }
        | Statement::ArrayElemsFill { .. }
        | Statement::SimdV128 { .. }
        | Statement::ValueDrop(_)
        | Statement::ValueRetain(_)
        | Statement::ValueKill(_) => false,
        Statement::Assign(place, rv) => {
            if matches!(
                place,
                Place::Global(_) | Place::Index { .. } | Place::Deref { .. }
            ) {
                return false;
            }
            if let Place::Field { base, .. } = place {
                let ok_base = new_locals.contains(base)
                    || (this_local == Some(*base) && cx.ctor_only.contains(&f.def));
                if !ok_base {
                    return false;
                }
            }
            rvalue_region_safe(cx, rv)
        }
    }
}

pub(super) fn rvalue_region_safe(cx: &mut SafeCx<'_>, rv: &Rvalue) -> bool {
    match rv {
        Rvalue::ObservedLoad(_) => false,
        Rvalue::Use(_)
        | Rvalue::Move { .. }
        | Rvalue::Select { .. }
        | Rvalue::Binary(_, _, _)
        | Rvalue::CheckedBinary(_, _, _)
        | Rvalue::Unary(_, _)
        | Rvalue::CheckedNeg(_)
        | Rvalue::StrLen(_)
        | Rvalue::StrByteSize(_)
        | Rvalue::CharAt(_, _, _)
        | Rvalue::ByteAt(_, _, _)
        | Rvalue::StrBytes(_)
        | Rvalue::LoadU8(_, _)
        | Rvalue::LoadU16(_, _)
        | Rvalue::HashCode(_)
        | Rvalue::ArrayLen(_)
        | Rvalue::Cast(_, _, _)
        | Rvalue::Discriminant { .. }
        | Rvalue::UnionField { .. }
        | Rvalue::IsType(_, _)
        | Rvalue::TypeName(_)
        | Rvalue::Tuple { .. }
        | Rvalue::EnumName { .. } => true,
        Rvalue::Call { callee, .. } => callee_safe(cx, callee),
        Rvalue::New { ty, ctor, .. } => {
            let Some(layout) = cx.mir.layouts.get(*ty) else {
                return false;
            };
            if layout.has_destructor() {
                return false;
            }
            if let Some(ctor) = ctor {
                if !cx.ctor_only.contains(&ctor.def) {
                    return false;
                }
                let constructors: Vec<_> = cx
                    .mir
                    .functions
                    .iter()
                    .filter(|f| f.def == ctor.def)
                    .collect();
                !constructors.is_empty() && constructors.into_iter().all(|f| region_safe(cx, f))
            } else {
                true
            }
        }
        Rvalue::UnionNew { ty, .. } => {
            cx.interner.is_niche_union(*ty)
                && cx
                    .mir
                    .layouts
                    .union(*ty)
                    .is_none_or(|u| !u.has_destructor())
        }
        Rvalue::ArrayNew { .. }
        | Rvalue::ArrayLit { .. }
        | Rvalue::ArrayRealloc { .. }
        | Rvalue::Concat(_)
        | Rvalue::ConcatInt { .. }
        | Rvalue::ToString(_)
        | Rvalue::ToBytes { .. }
        | Rvalue::FromBytes { .. }
        | Rvalue::IndirectCall { .. }
        | Rvalue::InterfaceCall { .. }
        | Rvalue::JsCall { .. }
        | Rvalue::FuncRef(_) => false,
    }
}

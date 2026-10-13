use super::*;

/// A parameter has a target only when every entry is a direct, agreeing call. Exported,
/// address-taken and constructor bodies have additional entry paths and cannot use this proof.
pub(super) fn infer(
    mir: &Mir,
    interner: &TypeInterner,
    fields: &BTreeMap<Field, Target>,
    intrinsics: &BTreeMap<DefId, IntrinsicOp>,
    escaped: &BTreeSet<TypeId>,
    constructors: &BTreeSet<DefId>,
) -> BTreeMap<Key, BTreeMap<Local, Target>> {
    let taken = crate::passes::funcbox_abi::address_taken(mir);
    let candidates: BTreeMap<_, _> = mir
        .functions
        .iter()
        .filter(|f| {
            !f.is_async
                && f.params
                    .iter()
                    .any(|&p| matches!(interner.kind(f.local_ty(p)), dream_types::TyKind::Func(..)))
                && !constructors.contains(&f.def)
                && !mir.exports.iter().any(|(def, _)| *def == f.def)
                && !taken.contains(&(f.def, f.instance.clone()))
        })
        .map(|f| ((f.def, f.instance.clone()), f))
        .collect();
    if candidates.is_empty() {
        return BTreeMap::new();
    }
    let empty = BTreeMap::new();
    let mut incoming: BTreeMap<(Key, usize), Option<Target>> = BTreeMap::new();
    for f in mir.functions.iter().chain(&mir.polls) {
        let relevant =
            |callee: &Callee| candidates.contains_key(&(callee.def, callee.args.clone()));
        if !f.blocks.iter().any(|block| {
            block.stmts.iter().any(|s| match s {
                Statement::Call { callee, .. } | Statement::Assign(_, Rvalue::Call { callee, .. }) => relevant(callee),
                _ => false,
            }) || matches!(&block.terminator, Terminator::TailCall { callee, .. } if relevant(callee))
        }) {
            continue;
        }
        let facts = local_targets(f, fields, intrinsics, escaped, &empty);
        let mut visit = |callee: &Callee, args: &[Operand]| {
            let key = (callee.def, callee.args.clone());
            let Some(body) = candidates.get(&key) else {
                return;
            };
            for (i, &p) in body.params.iter().enumerate() {
                if !matches!(
                    interner.kind(body.local_ty(p)),
                    dream_types::TyKind::Func(..)
                ) || body.locals[p.0 as usize].is_ref
                    || escaped.contains(&body.local_ty(p))
                    || body
                        .blocks
                        .iter()
                        .flat_map(|b| &b.stmts)
                        .any(|s| matches!(s, Statement::Assign(Place::Local(d), _) if *d == p))
                {
                    continue;
                }
                let target = args.get(i).and_then(|a| op_target(a, f, &facts, fields));
                incoming
                    .entry((key.clone(), i))
                    .and_modify(|prior| {
                        if *prior != target {
                            *prior = None;
                        }
                    })
                    .or_insert(target);
            }
        };
        for block in &f.blocks {
            for s in &block.stmts {
                if let Statement::Call { callee, args }
                | Statement::Assign(_, Rvalue::Call { callee, args }) = s
                {
                    visit(callee, args);
                }
            }
            if let Terminator::TailCall { callee, args } = &block.terminator {
                visit(callee, args);
            }
        }
    }
    let mut result: BTreeMap<Key, BTreeMap<Local, Target>> = BTreeMap::new();
    for ((key, i), target) in incoming {
        if let Some(target) = target {
            result
                .entry(key.clone())
                .or_default()
                .insert(candidates[&key].params[i], target);
        }
    }
    result
}

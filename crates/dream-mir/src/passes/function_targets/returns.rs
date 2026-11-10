use super::*;

/// Only a target shared by every return is useful. Unresolved recursive returns stay unknown;
/// discovering one branch never supplies a provisional fact to the others.
pub(super) fn infer(
    mir: &Mir,
    interner: &TypeInterner,
    fields: &BTreeMap<Field, Target>,
    intrinsics: &BTreeMap<DefId, IntrinsicOp>,
    escaped: &BTreeSet<TypeId>,
) -> BTreeMap<Key, Target> {
    let mut returns = BTreeMap::new();
    if mir.profile.is_debug() {
        return returns;
    }
    loop {
        let before = returns.len();
        for f in &mir.functions {
            let key = (f.def, f.instance.clone());
            if f.is_async
                || !matches!(interner.kind(f.ret), dream_types::TyKind::Func(..))
                || escaped.contains(&f.ret)
                || returns.contains_key(&key)
            {
                continue;
            }
            let locals = local_targets(f, fields, intrinsics, escaped, &returns, &BTreeMap::new());
            let dom = FunctionAnalyses::default().dominators(f);
            let definitions = definitions(f);
            let mut common = None;
            let mut complete = true;
            for (bi, block) in f.blocks.iter().enumerate() {
                let target = match &block.terminator {
                    Terminator::Return(Some(value)) => {
                        if !available(
                            value,
                            (BlockId(bi as u32), block.stmts.len()),
                            &definitions,
                            &dom,
                            intrinsics,
                            &mut BTreeSet::new(),
                        ) {
                            complete = false;
                            break;
                        }
                        op_target(value, f, &locals, fields)
                    }
                    Terminator::TailCall { callee, .. } => {
                        returns.get(&(callee.def, callee.args.clone())).cloned()
                    }
                    Terminator::Return(None) => None,
                    _ => continue,
                };
                let Some(target) = target else {
                    complete = false;
                    break;
                };
                if common.as_ref().is_some_and(|previous| previous != &target) {
                    complete = false;
                    break;
                }
                common = Some(target);
            }
            if complete && let Some(target) = common {
                returns.insert(key, target);
            }
        }
        if returns.len() == before {
            return returns;
        }
    }
}

type Definitions<'a> = BTreeMap<Local, (BlockId, usize, &'a Rvalue)>;

fn definitions(f: &MirFunction) -> Definitions<'_> {
    f.blocks
        .iter()
        .enumerate()
        .flat_map(|(bi, block)| {
            block
                .stmts
                .iter()
                .enumerate()
                .filter_map(move |(si, stmt)| match stmt {
                    Statement::Assign(Place::Local(local), rv) => {
                        Some((*local, (BlockId(bi as u32), si, rv)))
                    }
                    _ => None,
                })
        })
        .collect()
}

// A single assignment may still be conditional or occur after a dependent snapshot. Every
// code-identity source must dominate its actual read, not just the function's return.
fn available(
    op: &Operand,
    at: (BlockId, usize),
    definitions: &Definitions<'_>,
    dom: &crate::passes::analyses::DomTree,
    intrinsics: &BTreeMap<DefId, IntrinsicOp>,
    seen: &mut BTreeSet<Local>,
) -> bool {
    let Operand::Copy(Place::Local(local)) = op else {
        return matches!(op, Operand::Copy(Place::Field { .. }));
    };
    if !seen.insert(*local) {
        return false;
    }
    let Some(&(block, statement, rv)) = definitions.get(local) else {
        return false;
    };
    if !dom.dominates(block, at.0) || (block == at.0 && statement >= at.1) {
        return false;
    }
    let source = match rv {
        Rvalue::FuncRef(_) => return true,
        Rvalue::Use(value) | Rvalue::Cast(value, ..) => Some(value),
        Rvalue::Call { callee, args }
            if matches!(
                intrinsics.get(&callee.def),
                Some(IntrinsicOp::FuncboxNew | IntrinsicOp::FuncboxFuncidx)
            ) =>
        {
            args.first()
        }
        Rvalue::Call { .. } => return true,
        _ => None,
    };
    source.is_some_and(|source| {
        available(
            source,
            (block, statement),
            definitions,
            dom,
            intrinsics,
            seen,
        )
    })
}

//! Resolve function boxes and closed, constructor-initialized function fields.
//! A field invariant is usable only for types confined to typed, closed-world code.

use super::FunctionAnalyses;
use crate::{
    BlockId, Callee, Local, Mir, MirFunction, Operand, Place, Rvalue, Statement, Terminator,
};
use dream_abi::intrinsics::IntrinsicOp;
use dream_types::{DefId, TypeId, TypeInterner};
use std::collections::{BTreeMap, BTreeSet};

use crate::analysis::function_boxes::{self as escape, operand_type};
mod params;

type Field = (TypeId, usize);
type Key = (DefId, Vec<TypeId>);
pub(super) const STAGE: &str = "function-targets";

#[derive(Clone, Debug, PartialEq, Eq)]
struct Target {
    callee: Callee,
    boxed: bool,
}

pub(super) fn run(mir: &mut Mir, interner: &TypeInterner) -> bool {
    let intrinsics: BTreeMap<_, _> = mir
        .intrinsics
        .iter()
        .filter_map(|(d, key)| IntrinsicOp::from_key(key).map(|op| (*d, op)))
        .collect();
    let borrowed_results: BTreeSet<_> = mir
        .functions
        .iter()
        .filter(|f| f.borrowed_result)
        .map(|f| f.def)
        .collect();
    let escaped = escape::box_types(mir, interner);
    let fields = field_targets(mir, interner, &intrinsics, &escaped);
    let ctors: BTreeSet<_> = mir
        .functions
        .iter()
        .chain(&mir.polls)
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.stmts)
        .filter_map(|s| match s {
            Statement::Assign(
                _,
                Rvalue::New {
                    ctor: Some(ctor), ..
                },
            ) => Some(ctor.def),
            _ => None,
        })
        .collect();
    let empty = BTreeMap::new();
    let empty_parameters = BTreeMap::new();
    let parameters = params::infer(mir, interner, &fields, &intrinsics, &escaped, &ctors);
    let mut changed = false;
    for f in mir.functions.iter_mut().chain(mir.polls.iter_mut()) {
        if !f.blocks.iter().flat_map(|b| &b.stmts).any(|s| {
            matches!(
                s,
                Statement::IndirectCall { .. } | Statement::Assign(_, Rvalue::IndirectCall { .. })
            )
        }) {
            continue;
        }
        let facts = local_targets(
            f,
            if ctors.contains(&f.def) {
                &empty
            } else {
                &fields
            },
            &intrinsics,
            &escaped,
            parameters
                .get(&(f.def, f.instance.clone()))
                .unwrap_or(&empty_parameters),
        );
        let dom = FunctionAnalyses::default().dominators(f);
        let mut definitions = BTreeMap::new();
        for (bi, block) in f.blocks.iter().enumerate() {
            for (si, stmt) in block.stmts.iter().enumerate() {
                if let Statement::Assign(Place::Local(local), _) = stmt {
                    definitions.insert(*local, (BlockId(bi as u32), si));
                }
            }
        }
        for (bi, block) in f.blocks.iter_mut().enumerate() {
            for (si, stmt) in block.stmts.iter_mut().enumerate() {
                let (target, args) = match stmt {
                    Statement::Assign(_, Rvalue::IndirectCall { target, args, .. })
                    | Statement::IndirectCall { target, args, .. } => (target, args),
                    _ => continue,
                };
                let Operand::Copy(Place::Local(local)) = target else {
                    continue;
                };
                let Some(t) = facts.get(local).filter(|t| !t.boxed) else {
                    continue;
                };
                let Some(&(db, ds)) = definitions.get(local) else {
                    continue;
                };
                if !dom.dominates(db, BlockId(bi as u32)) || (db.0 as usize == bi && ds >= si) {
                    continue;
                }
                if borrowed_results.contains(&t.callee.def) {
                    continue;
                }
                let mut callee = t.callee.clone();
                // Function-value targets retain their +0 ABI after becoming direct calls.
                callee.take_params = vec![false; args.len()];
                let args = args.clone();
                match stmt {
                    Statement::Assign(_, rv) => *rv = Rvalue::Call { callee, args },
                    _ => *stmt = Statement::Call { callee, args },
                }
                changed = true;
            }
        }
    }
    changed
}

fn op_target(
    o: &Operand,
    f: &MirFunction,
    locals: &BTreeMap<Local, Target>,
    fields: &BTreeMap<Field, Target>,
) -> Option<Target> {
    match o {
        Operand::Copy(Place::Local(l)) => locals.get(l).cloned(),
        Operand::Copy(Place::Field { base, field }) => {
            fields.get(&(f.local_ty(*base), *field)).cloned()
        }
        _ => None,
    }
}

fn rv_target(
    rv: &Rvalue,
    f: &MirFunction,
    locals: &BTreeMap<Local, Target>,
    fields: &BTreeMap<Field, Target>,
    intrinsics: &BTreeMap<DefId, IntrinsicOp>,
) -> Option<Target> {
    match rv {
        Rvalue::FuncRef(callee) => Some(Target {
            callee: callee.clone(),
            boxed: false,
        }),
        Rvalue::Use(o) | Rvalue::Cast(o, _, _) => op_target(o, f, locals, fields),
        Rvalue::Call { callee, args } => match intrinsics.get(&callee.def) {
            Some(IntrinsicOp::FuncboxNew) => {
                let mut target = op_target(args.first()?, f, locals, fields)?;
                if target.boxed {
                    return None;
                }
                target.boxed = true;
                Some(target)
            }
            Some(IntrinsicOp::FuncboxFuncidx) => {
                let mut target = op_target(args.first()?, f, locals, fields)?;
                if !target.boxed {
                    return None;
                }
                target.boxed = false;
                Some(target)
            }
            _ => None,
        },
        _ => None,
    }
}

fn local_targets(
    f: &MirFunction,
    fields: &BTreeMap<Field, Target>,
    intrinsics: &BTreeMap<DefId, IntrinsicOp>,
    escaped: &BTreeSet<TypeId>,
    parameters: &BTreeMap<Local, Target>,
) -> BTreeMap<Local, Target> {
    let mut defs: BTreeMap<Local, Vec<&Rvalue>> = BTreeMap::new();
    for stmt in f.blocks.iter().flat_map(|b| &b.stmts) {
        if let Statement::Assign(Place::Local(l), rv) = stmt {
            defs.entry(*l).or_default().push(rv);
        }
    }
    let mut facts = parameters.clone();
    loop {
        let mut changed = false;
        for (&local, defs) in &defs {
            if !escaped.contains(&f.local_ty(local))
                && defs.len() == 1
                && !facts.contains_key(&local)
                && let Some(target) = rv_target(defs[0], f, &facts, fields, intrinsics)
            {
                facts.insert(local, target);
                changed = true;
            }
        }
        if !changed {
            return facts;
        }
    }
}

fn field_targets(
    mir: &Mir,
    interner: &TypeInterner,
    intrinsics: &BTreeMap<DefId, IntrinsicOp>,
    escaped: &BTreeSet<TypeId>,
) -> BTreeMap<Field, Target> {
    if !mir.exports.is_empty() {
        return BTreeMap::new();
    }
    let bodies: BTreeMap<Key, _> = mir
        .functions
        .iter()
        .map(|f| ((f.def, f.instance.clone()), f))
        .collect();
    let mut constructors: BTreeMap<TypeId, BTreeSet<Key>> = BTreeMap::new();
    let mut rejected = BTreeSet::new();
    for f in mir.functions.iter().chain(&mir.polls) {
        for stmt in f.blocks.iter().flat_map(|b| &b.stmts) {
            match stmt {
                // Untyped writes can invalidate any typed field invariant.
                Statement::Assign(Place::Deref { .. }, _)
                | Statement::RawStore { .. }
                | Statement::ForceFree(_) => {
                    return BTreeMap::new();
                }
                Statement::Assign(_, Rvalue::New { ty, ctor, .. }) => {
                    if let Some(ctor) = ctor {
                        let key = (*ty, ctor.def);
                        if let Some(body) = mir.functions.iter().find(|body| {
                            body.def == key.1
                                && body
                                    .params
                                    .first()
                                    .is_some_and(|this| body.local_ty(*this) == key.0)
                        }) {
                            constructors
                                .entry(*ty)
                                .or_default()
                                .insert((body.def, body.instance.clone()));
                        } else {
                            rejected.insert(*ty);
                        }
                    } else {
                        rejected.insert(*ty);
                    }
                }
                _ => {}
            }
            match stmt {
                Statement::Assign(_, Rvalue::Cast(_, from, to))
                    if from != to && interner.is_reference(*from) =>
                {
                    rejected.insert(*from);
                    rejected.insert(*to);
                }
                Statement::Assign(
                    Place::Field { .. } | Place::Index { .. } | Place::Global(_),
                    Rvalue::Use(o),
                ) => {
                    if let Some(ty) = operand_type(f, o) {
                        rejected.insert(ty);
                    }
                }
                Statement::Assign(
                    _,
                    Rvalue::ArrayLit { elems: args, .. }
                    | Rvalue::New { args, .. }
                    | Rvalue::Tuple { elems: args, .. }
                    | Rvalue::UnionNew { args, .. },
                ) => {
                    for o in args {
                        if let Some(ty) = operand_type(f, o) {
                            rejected.insert(ty);
                        }
                    }
                }
                Statement::Call { callee, args }
                | Statement::Assign(_, Rvalue::Call { callee, args })
                    if !bodies.contains_key(&(callee.def, callee.args.clone()))
                        && !intrinsics.contains_key(&callee.def) =>
                {
                    rejected.insert(callee.ret);
                    for o in args {
                        if let Some(ty) = operand_type(f, o) {
                            rejected.insert(ty);
                        }
                    }
                }
                Statement::IndirectCall { args, .. }
                | Statement::Assign(
                    _,
                    Rvalue::IndirectCall { args, .. } | Rvalue::InterfaceCall { args, .. },
                ) => {
                    for o in args {
                        if let Some(ty) = operand_type(f, o) {
                            rejected.insert(ty);
                        }
                    }
                }
                _ => {}
            }
        }
        for block in &f.blocks {
            if let Terminator::Return(Some(o)) = &block.terminator
                && let Some(ty) = operand_type(f, o)
            {
                rejected.insert(ty);
            }
        }
    }
    let mut fields = BTreeMap::new();
    for (ty, ctors) in constructors {
        if rejected.contains(&ty) {
            continue;
        }
        let mut common: Option<BTreeMap<Field, Target>> = None;
        for key in &ctors {
            let f = bodies[key];
            let this = f.params[0];
            // Single-block constructors finish initialization before exposing `this` to any call.
            if f.blocks.len() != 1 {
                common = None;
                break;
            }
            let locals = local_targets(f, &BTreeMap::new(), intrinsics, escaped, &BTreeMap::new());
            let mut initialized = BTreeMap::new();
            for stmt in &f.blocks[0].stmts {
                match stmt {
                    Statement::Assign(Place::Field { base, field }, rv) if *base == this => {
                        if escape::result_type(rv, f).is_some_and(|ty| escaped.contains(&ty)) {
                            continue;
                        }
                        if let Some(t) = rv_target(rv, f, &locals, &BTreeMap::new(), intrinsics)
                            .filter(|t| t.boxed)
                        {
                            initialized.insert((ty, *field), t);
                        }
                    }
                    Statement::Call { args, .. }
                    | Statement::Assign(_, Rvalue::Call { args, .. })
                        if args.iter().any(|o| operand_type(f, o) == Some(ty)) =>
                    {
                        initialized.clear();
                        break;
                    }
                    _ => {}
                }
            }
            common = Some(match common {
                None => initialized,
                Some(previous) => previous
                    .into_iter()
                    .filter(|(key, value)| initialized.get(key) == Some(value))
                    .collect(),
            });
        }
        if let Some(common) = common {
            fields.extend(common);
        }
    }
    // Every live typed write must agree; setters and borrowed constructor arguments stay dynamic.
    for f in mir.functions.iter().chain(&mir.polls) {
        let locals = local_targets(f, &BTreeMap::new(), intrinsics, escaped, &BTreeMap::new());
        for stmt in f.blocks.iter().flat_map(|b| &b.stmts) {
            if let Statement::Assign(Place::Field { base, field }, rv) = stmt {
                let key = (f.local_ty(*base), *field);
                if fields.contains_key(&key)
                    && rv_target(rv, f, &locals, &BTreeMap::new(), intrinsics).as_ref()
                        != fields.get(&key)
                {
                    fields.remove(&key);
                }
            }
        }
    }
    fields
}

#[cfg(test)]
mod tests;

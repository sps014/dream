use super::*;
use std::collections::VecDeque;

#[derive(Clone, Default, PartialEq, Eq)]
enum Fact {
    #[default]
    Pending,
    Target(Target),
    Unknown,
}

impl Fact {
    fn join(&mut self, incoming: Self) -> bool {
        let next = match (&*self, incoming) {
            (_, Fact::Pending) | (Fact::Unknown, _) => return false,
            (Fact::Pending, incoming) => incoming,
            (Fact::Target(a), Fact::Target(b)) if a == &b => return false,
            _ => Fact::Unknown,
        };
        *self = next;
        true
    }
}

/// Pending forwarding edges acquire their entry's target before recursive callers are joined.
/// Unknown entries and disagreements poison downstream facts; an unseeded cycle proves nothing.
/// Debug keeps its existing caller-local target inference.
pub(super) fn infer(
    mir: &Mir,
    interner: &TypeInterner,
    fields: &BTreeMap<Field, Target>,
    intrinsics: &BTreeMap<DefId, IntrinsicOp>,
    escaped: &BTreeSet<TypeId>,
    constructors: &BTreeSet<DefId>,
) -> BTreeMap<Key, BTreeMap<Local, Target>> {
    let taken = crate::passes::funcbox_abi::address_taken(mir);
    let bodies: Vec<_> = mir.functions.iter().chain(&mir.polls).collect();
    let mut indices = BTreeMap::new();
    let mut states = BTreeMap::new();
    for (index, f) in mir.functions.iter().enumerate() {
        let key = (f.def, f.instance.clone());
        if f.is_async
            || constructors.contains(&f.def)
            || mir.exports.iter().any(|(def, _)| *def == f.def)
            || taken.contains(&key)
        {
            continue;
        }
        let parameters: BTreeMap<_, _> = f
            .params
            .iter()
            .filter(|&&p| {
                matches!(interner.kind(f.local_ty(p)), dream_types::TyKind::Func(..))
                    && !f.locals[p.0 as usize].is_ref
                    && !escaped.contains(&f.local_ty(p))
                    && !f
                        .blocks
                        .iter()
                        .flat_map(|b| &b.stmts)
                        .any(|s| matches!(s, Statement::Assign(Place::Local(d), _) if *d == p))
            })
            .map(|&p| (p, Fact::Pending))
            .collect();
        if !parameters.is_empty() {
            indices.insert(key.clone(), index);
            states.insert(key, parameters);
        }
    }
    if states.is_empty() {
        return BTreeMap::new();
    }
    let relevant: Vec<_> = bodies.iter().map(|f| {
        let candidate = |callee: &Callee| states.contains_key(&(callee.def, callee.args.clone()));
        f.blocks.iter().any(|block| {
            block.stmts.iter().any(|statement| match statement {
                Statement::Call { callee, .. } | Statement::Assign(_, Rvalue::Call { callee, .. }) => candidate(callee),
                _ => false,
            }) || matches!(&block.terminator, Terminator::TailCall { callee, .. } if candidate(callee))
        })
    }).collect();
    let mut queue: VecDeque<_> = relevant
        .iter()
        .enumerate()
        .filter_map(|(index, &yes)| yes.then_some(index))
        .collect();
    let mut queued = relevant.clone();
    let mut evaluations = 0;
    loop {
        while let Some(index) = queue.pop_front() {
            queued[index] = false;
            evaluations += 1;
            let f = bodies[index];
            let key = (f.def, f.instance.clone());
            let known = states
                .get(&key)
                .into_iter()
                .flatten()
                .filter_map(|(&p, fact)| match fact {
                    Fact::Target(target) if !mir.profile.is_debug() => Some((p, target.clone())),
                    _ => None,
                })
                .collect();
            let pending = states
                .get(&key)
                .into_iter()
                .flatten()
                .filter_map(|(&p, fact)| {
                    (!mir.profile.is_debug() && matches!(fact, Fact::Pending)).then_some(p)
                })
                .collect();
            let facts = local_targets(f, fields, intrinsics, escaped, &known);
            let pending = pending_locals(f, pending, intrinsics, escaped);
            let mut visit = |callee: &Callee, args: &[Operand]| {
                let key = (callee.def, callee.args.clone());
                let Some(parameters) = states.get_mut(&key) else {
                    return;
                };
                let body_index = indices[&key];
                for (&p, fact) in parameters {
                    let position = bodies[body_index]
                        .params
                        .iter()
                        .position(|parameter| *parameter == p)
                        .expect("callback parameter belongs to callee");
                    let incoming = match args.get(position) {
                        Some(arg) => match op_target(arg, f, &facts, fields) {
                            Some(target) => Fact::Target(target),
                            None if matches!(arg, Operand::Copy(Place::Local(l)) if pending.contains(l)) => {
                                Fact::Pending
                            }
                            None => Fact::Unknown,
                        },
                        None => Fact::Unknown,
                    };
                    if fact.join(incoming) && relevant[body_index] && !queued[body_index] {
                        queued[body_index] = true;
                        queue.push_back(body_index);
                    }
                }
            };
            for block in &f.blocks {
                for statement in &block.stmts {
                    if let Statement::Call { callee, args }
                    | Statement::Assign(_, Rvalue::Call { callee, args }) = statement
                    {
                        visit(callee, args);
                    }
                }
                if let Terminator::TailCall { callee, args } = &block.terminator {
                    visit(callee, args);
                }
            }
        }
        for (key, parameters) in &mut states {
            for fact in parameters.values_mut() {
                if matches!(fact, Fact::Pending) {
                    *fact = Fact::Unknown;
                    let index = indices[key];
                    if relevant[index] && !queued[index] {
                        queued[index] = true;
                        queue.push_back(index);
                    }
                }
            }
        }
        if queue.is_empty() {
            break;
        }
    }
    tracing::debug!(
        evaluations,
        functions = states.len(),
        "closed callback parameter targets"
    );
    states
        .into_iter()
        .map(|(key, parameters)| {
            let known = parameters
                .into_iter()
                .filter_map(|(p, fact)| match fact {
                    Fact::Target(target) => Some((p, target)),
                    _ => None,
                })
                .collect();
            (key, known)
        })
        .collect()
}

fn pending_locals(
    f: &MirFunction,
    mut pending: BTreeSet<Local>,
    intrinsics: &BTreeMap<DefId, IntrinsicOp>,
    escaped: &BTreeSet<TypeId>,
) -> BTreeSet<Local> {
    if pending.is_empty() {
        return pending;
    }
    let mut definitions = BTreeMap::<Local, Vec<&Rvalue>>::new();
    for statement in f.blocks.iter().flat_map(|b| &b.stmts) {
        if let Statement::Assign(Place::Local(local), value) = statement {
            definitions.entry(*local).or_default().push(value);
        }
    }
    loop {
        let before = pending.len();
        for (&local, definitions) in &definitions {
            if definitions.len() != 1 || escaped.contains(&f.local_ty(local)) {
                continue;
            }
            let source = match definitions[0] {
                Rvalue::Use(op) | Rvalue::Cast(op, ..) => Some(op),
                Rvalue::Call { callee, args }
                    if matches!(
                        intrinsics.get(&callee.def),
                        Some(IntrinsicOp::FuncboxNew | IntrinsicOp::FuncboxFuncidx)
                    ) =>
                {
                    args.first()
                }
                _ => None,
            };
            if matches!(source, Some(Operand::Copy(Place::Local(l))) if pending.contains(l)) {
                pending.insert(local);
            }
        }
        if pending.len() == before {
            return pending;
        }
    }
}

use crate::Const;
use crate::Local;
use crate::Operand;
use crate::Place;
use crate::Rvalue;
use crate::Statement;

pub(crate) fn move_source(rvalue: &Rvalue, is_owned_ref: &dyn Fn(u32) -> bool) -> Option<Local> {
    match rvalue {
        Rvalue::Use(Operand::Copy(Place::Local(src))) if is_owned_ref(src.0) => Some(*src),
        _ => None,
    }
}

pub(crate) fn sink_call_args(stmt: &Statement) -> Option<(Vec<bool>, &[Operand])> {
    match stmt {
        Statement::Call { callee, args } => Some((callee.take_params.clone(), args)),
        Statement::Assign(_, Rvalue::Call { callee, args, .. }) => {
            Some((callee.take_params.clone(), args))
        }
        // A constructor arg is a sink unless the ctor declares that parameter `borrow`/`ref`, in
        // which case the ctor body's own field store does the retain. Flags that don't line up with
        // the args mean the ctor's declaration wasn't resolvable, and unknown must retain —
        // under-retaining frees a live object.
        Statement::Assign(
            _,
            Rvalue::New {
                ctor: Some(c),
                args,
                ..
            },
        ) => {
            let flags = if c.take_params.len() == args.len() {
                c.take_params.clone()
            } else {
                vec![true; args.len()]
            };
            Some((flags, args))
        }
        // FuncboxAbi makes address-taken parameters borrowed (+0), including reference-bearing
        // values. The caller keeps ownership; a callee retains only when ownership escapes.
        Statement::IndirectCall { args, .. } => Some((vec![false; args.len()], args)),
        Statement::Assign(_, Rvalue::IndirectCall { args, .. }) => {
            Some((vec![false; args.len()], args))
        }
        // Interface implementations use the same borrowed ABI as indirect targets.
        Statement::InterfaceCall { args, .. } => Some((vec![false; args.len()], args)),
        Statement::Assign(_, Rvalue::InterfaceCall { args, .. }) => {
            Some((vec![false; args.len()], args))
        }
        _ => None,
    }
}

pub(crate) fn take_arg_effects(
    stmt: &Statement,
    is_owned_ref: &dyn Fn(u32) -> bool,
    local_is_ref: &[bool],
    is_move: impl Fn(u32) -> bool,
) -> (Vec<Statement>, Vec<Statement>) {
    let Some((take_params, args)) = sink_call_args(stmt) else {
        return (Vec::new(), Vec::new());
    };
    let mut retains = Vec::new();
    let mut nulls = Vec::new();
    // A local handed to several `take` parameters of one call (`f(xs, xs)`) owes the callee one
    // reference per parameter, but the caller holds a single one. Counting the occurrences first
    // means a move can null the source once and retain for the rest; nulling per occurrence would
    // transfer the same reference twice and the callee would release it twice.
    let mut take_counts: std::collections::BTreeMap<u32, u32> = std::collections::BTreeMap::new();
    for (i, arg) in args.iter().enumerate() {
        if !take_params.get(i).copied().unwrap_or(false) {
            continue;
        }
        match arg {
            Operand::Copy(Place::Local(l))
                if local_is_ref.get(l.0 as usize).copied().unwrap_or(false) =>
            {
                *take_counts.entry(l.0).or_insert(0) += 1;
            }
            Operand::Copy(Place::Field { .. })
            | Operand::Copy(Place::Index { .. })
            | Operand::Const(Const::Str(_)) => {
                retains.push(Statement::Retain(arg.clone()));
            }
            _ => {}
        }
    }
    for (local, n) in take_counts {
        let moved = is_owned_ref(local) && is_move(local);
        let retain_count = if moved { n - 1 } else { n };
        for _ in 0..retain_count {
            retains.push(Statement::Retain(Operand::Copy(Place::Local(
                crate::Local(local),
            ))));
        }
        if moved {
            nulls.push(Statement::Assign(
                Place::Local(crate::Local(local)),
                Rvalue::Use(Operand::Const(Const::Null)),
            ));
        }
    }
    (retains, nulls)
}

pub(crate) fn take_owned_arg_locals(
    stmt: &Statement,
    is_owned_ref: &dyn Fn(u32) -> bool,
) -> Vec<u32> {
    let Some((take_params, args)) = sink_call_args(stmt) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        if !take_params.get(i).copied().unwrap_or(false) {
            continue;
        }
        if let Operand::Copy(Place::Local(l)) = arg
            && is_owned_ref(l.0)
        {
            out.push(l.0);
        }
    }
    out
}

/// Borrowed or taken call arguments may be retained by the callee; Unique last-use destroy is unsound.
pub(crate) fn call_escape_locals(stmt: &Statement, is_owned_ref: &dyn Fn(u32) -> bool) -> Vec<u32> {
    let Some((_, args)) = sink_call_args(stmt) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for arg in args {
        if let Operand::Copy(Place::Local(l)) = arg
            && is_owned_ref(l.0)
        {
            out.push(l.0);
        }
    }
    out
}

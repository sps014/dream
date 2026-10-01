//! One call-effect model shared by function summaries and intraprocedural region checking.

use super::returns::{self, Facts, Returns, Sources};
use crate::{Const, Operand, Rvalue, Statement};

pub(super) fn effects(stmt: &Statement, summaries: &Returns) -> Option<(Vec<Operand>, Facts)> {
    match stmt {
        Statement::Call { callee, args } | Statement::Assign(_, Rvalue::Call { callee, args }) => {
            Some((
                args.clone(),
                returns::call_facts(callee, args.len(), summaries),
            ))
        }
        Statement::Assign(
            _,
            Rvalue::New {
                ctor: Some(ctor),
                args,
                ty,
                ..
            },
        ) => {
            let mut actual = vec![Operand::Const(Const::Null)];
            actual.extend(args.iter().cloned());
            let callee = crate::Callee {
                def: ctor.def,
                args: vec![],
                ret: *ty,
                take_params: vec![],
            };
            let mut facts = returns::call_facts(&callee, actual.len(), summaries);
            facts.writes.remove(&0);
            facts.escaped.fresh |= facts.escaped.params.remove(&0);
            Some((actual, facts))
        }
        Statement::Assign(
            _,
            rv @ (Rvalue::IndirectCall { .. }
            | Rvalue::InterfaceCall { .. }
            | Rvalue::JsCall { .. }),
        ) => Some(opaque(super::operands::rvalue_local_operands(rv))),
        Statement::IndirectCall { .. }
        | Statement::InterfaceCall { .. }
        | Statement::JsCall { .. } => Some(opaque(super::operands::other_stmt_locals(stmt))),
        _ => None,
    }
}

fn opaque(locals: Vec<u32>) -> (Vec<Operand>, Facts) {
    let args: Vec<_> = locals
        .into_iter()
        .map(|l| Operand::Copy(crate::Place::Local(crate::Local(l))))
        .collect();
    let sources = Sources {
        fresh: true,
        params: (0..args.len()).collect(),
    };
    let facts = Facts {
        result: sources.clone(),
        writes: (0..args.len()).map(|i| (i, sources.clone())).collect(),
        escaped: sources,
    };
    (args, facts)
}

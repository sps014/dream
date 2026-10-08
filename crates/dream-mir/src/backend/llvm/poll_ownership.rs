//! Frame ownership transfers preserve captured environments and consumed call arguments.

use super::fx::{Fx, V};
use super::ir::Value;
use crate::{Const, Local, Operand, Place, Rvalue, Statement};

pub(super) fn captures_environment(body: &crate::MirFunction) -> bool {
    // Global zero is the reserved closure environment, never an application global.
    body.blocks
        .iter()
        .flat_map(|block| &block.stmts)
        .any(|statement| {
            let mut found = false;
            crate::visit::stmt_operands_mut(&mut statement.clone(), &mut |operand| {
                found |= matches!(operand, Operand::Copy(Place::Global(crate::Global(0))));
            });
            found
        })
}

fn ownership_step(statement: &Statement) -> bool {
    matches!(
        statement,
        Statement::Nop
            | Statement::SourceLine(_)
            | Statement::DebugLine(_)
            | Statement::Retain(_)
            | Statement::ValueRetain(_)
            | Statement::ValueKill(_)
            | Statement::Assign(
                Place::Local(_),
                Rvalue::Use(_)
                    | Rvalue::Move { cast: None, .. }
                    | Rvalue::UnionField { .. }
                    | Rvalue::UnionNew { .. }
                    | Rvalue::Select { .. }
            )
    )
}

impl Fx<'_, '_> {
    pub(super) fn release_poll_environment(&mut self) {
        let Some(offset) = self.poll_environment else {
            return;
        };
        let owner = V::u(self.self_.clone().unwrap());
        let at = self.addr(&owner, offset as i64);
        let env = self.load_ty(self.h(), &at, 8, true);
        self.store_ty(&self.h(), &at, &V::s(Value::zero(self.h())), 8);
        self.call("dream_release_closure_env", &[env]);
    }
    pub(super) fn poll_ownership_group(
        &mut self,
        statements: &[Statement],
        start: usize,
    ) -> Option<usize> {
        if self.poll_offsets.is_empty() {
            return None;
        }
        let cast = match &statements[start] {
            Statement::Assign(
                Place::Local(local),
                value @ (Rvalue::Cast(..) | Rvalue::Move { cast: Some(_), .. }),
            ) if self.poll_owned(*local) => Some((*local, value)),
            _ => None,
        };
        if cast.is_none() && !ownership_step(&statements[start]) {
            return None;
        }
        // Cast evaluation can enter foreign code; publish its result only after it returns.
        let cast = cast.map(|(local, value)| {
            let result = self.rvalue(value, Some(self.f.local_ty(local)));
            (local, value, result)
        });
        let next = start + 1;
        let end = statements[next..]
            .iter()
            .position(|s| !ownership_step(s))
            .map_or(statements.len(), |n| next + n);
        // Copies acquire their token after assignment; moves clear the previous slot after
        // assignment. Neither intermediate frame snapshot is a valid strong-edge graph.
        if let Some((local, value, result)) = cast {
            self.store(&Place::Local(local), value, result);
        } else {
            self.stmt(&statements[start]);
        }
        for statement in &statements[next..end] {
            self.stmt(statement);
        }
        Some(end - start)
    }

    pub(super) fn prepare_poll_transfer(&mut self, statements: &[Statement], start: usize) {
        if self.poll_offsets.is_empty() {
            return;
        }
        let taken: Vec<Local> = match &statements[start] {
            Statement::Call { callee, args }
            | Statement::Assign(_, Rvalue::Call { callee, args }) => {
                taken_locals(&callee.take_params, args, false)
            }
            Statement::Assign(
                _,
                Rvalue::New {
                    ctor: Some(ctor),
                    args,
                    ..
                },
            ) => taken_locals(&ctor.take_params, args, true),
            Statement::Assign(_, Rvalue::Move { src, .. }) => vec![*src],
            _ => return,
        };
        let moved: Vec<Local> = statements[start + 1..]
            .iter()
            // Rebinding stages the return token in a temporary, then releases the old
            // destination before moving the result and nulling consumed arguments.
            .take_while(|s| {
                ownership_step(s) || matches!(s, Statement::Release(_) | Statement::ValueDrop(_))
            })
            .filter_map(|s| match s {
                Statement::Assign(
                    Place::Local(local),
                    Rvalue::Use(Operand::Const(Const::Null)),
                ) if self.poll_owned(*local) && taken.contains(local) => Some(*local),
                _ => None,
            })
            .collect();
        if moved.is_empty() {
            return;
        }
        // The active stack still owns these tokens until the callee consumes them. Remove
        // the frame edges before application code can release or abandon the frame.
        for local in moved {
            self.clear_poll_edge(local);
        }
    }
}

fn taken_locals(flags: &[bool], args: &[Operand], constructor: bool) -> Vec<Local> {
    args.iter()
        .enumerate()
        .filter_map(|(i, argument)| {
            let takes = (constructor && flags.len() != args.len())
                || flags.get(i).copied().unwrap_or(false);
            match argument {
                Operand::Copy(Place::Local(local)) if takes => Some(*local),
                _ => None,
            }
        })
        .collect()
}

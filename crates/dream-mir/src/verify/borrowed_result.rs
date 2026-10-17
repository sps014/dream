//! The +0 getter ABI is valid only for strong slot reads rooted in a borrowed receiver.

use crate::analysis::effects::ModRefTable;
use crate::{Const, Local, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_hir::LayoutTable;
use dream_types::TypeInterner;
use indexmap::IndexSet;

pub(crate) fn prepare(
    f: &mut MirFunction,
    i: &TypeInterner,
    layouts: &LayoutTable,
    effects: &ModRefTable,
) {
    if !f.borrowed_result {
        return;
    }
    assert!(!f.is_async, "borrowed-result getter cannot suspend");
    let receiver = *f.params.first().expect("borrowed-result getter receiver");
    assert!(
        !f.locals[receiver.0 as usize].is_take,
        "getter receiver must be borrowed"
    );
    assert!(
        f.params
            .iter()
            .all(|p| !i.is_rc_tracked(f.local_ty(*p)) || !f.locals[p.0 as usize].is_take),
        "borrowed-result getters cannot release owning arguments"
    );
    let receiver_roots = aliases(f, layouts, IndexSet::from([receiver]));
    let borrowed: IndexSet<_> = f
        .params
        .iter()
        .copied()
        .filter(|p| !f.locals[p.0 as usize].is_take)
        .collect();
    let rooted = aliases(f, layouts, borrowed.clone());
    let direct = direct_aliases(f, borrowed);
    let live_out = crate::passes::rc::liveness::live_out(f);
    for (bi, block) in f.blocks.iter().enumerate() {
        for (si, stmt) in block.stmts.iter().enumerate() {
            let diverges = matches!(block.terminator, Terminator::Unreachable);
            assert!(
                diverges
                    || matches!(
                        stmt,
                        Statement::Assign(Place::Local(_), _)
                            | Statement::SourceLine(_)
                            | Statement::DebugLine(_)
                            | Statement::Call { .. }
                            | Statement::Nop
                    ),
                "borrowed-result getter must not mutate or invoke cleanup: {} {:?}",
                f.name,
                stmt
            );
            if let Statement::Assign(Place::Local(dest), _) = stmt {
                assert!(
                    !i.is_rc_tracked(f.local_ty(*dest)) || rooted.contains(dest),
                    "getter cannot construct an unrelated owning local"
                );
            }
            let call = match stmt {
                Statement::Assign(_, Rvalue::Call { callee, .. })
                | Statement::Call { callee, .. } => Some(callee),
                _ => None,
            };
            if let Some(callee) = call
                && !diverges
            {
                assert!(
                    callee.borrowed_result
                        || (!i.is_rc_tracked(callee.ret) && !i.is_value_type(callee.ret)),
                    "getter may only produce borrowed references"
                );
                if !effects.call(callee).is_quiet() {
                    let live_snapshot = rooted.iter().filter(|local| {
                        !direct.contains(*local) && (i.is_rc_tracked(f.local_ty(**local)) || i.is_value_type(f.local_ty(**local)))
                    }).any(|local| {
                        !matches!(stmt, Statement::Assign(Place::Local(dest), _) if dest == local)
                            && (crate::passes::rc::liveness::live_after_stmt(f, &live_out, bi, si, local.0)
                                || crate::passes::rc::stmt_reads_local(stmt, local.0))
                    });
                    assert!(
                        !live_snapshot,
                        "slot snapshot cannot cross an effectful getter call"
                    );
                }
            }
        }
        if let Terminator::Return(Some(op)) = &block.terminator {
            assert!(
                matches!(op, Operand::Const(Const::Null))
                    || matches!(op, Operand::Copy(place) if base_ok(f, layouts, place, &receiver_roots)),
                "borrowed-result getter must return a receiver slot: {}",
                f.name
            );
        }
    }
    for local in rooted {
        if !f.params.contains(&local) {
            if i.is_rc_tracked(f.local_ty(local)) {
                f.locals[local.0 as usize].is_cursor = true;
            } else if i.is_value_type(f.local_ty(local)) {
                f.locals[local.0 as usize].borrows_refs = true;
            }
        }
    }
}

fn base_ok(f: &MirFunction, layouts: &LayoutTable, place: &Place, roots: &IndexSet<Local>) -> bool {
    match place {
        Place::Local(l) | Place::Index { base: l, .. } => roots.contains(l),
        Place::Field { base, field } => {
            roots.contains(base)
                && layouts
                    .get(f.local_ty(*base))
                    .and_then(|l| l.fields.get(*field))
                    .is_some_and(|field| !field.is_weak && !field.is_unowned)
        }
        _ => false,
    }
}

fn aliases(f: &MirFunction, layouts: &LayoutTable, mut roots: IndexSet<Local>) -> IndexSet<Local> {
    loop {
        let mut changed = false;
        for local in 0..f.locals.len() {
            let dest = Local(local as u32);
            if roots.contains(&dest) {
                continue;
            }
            let mut defined = false;
            let valid = f.blocks.iter().flat_map(|b| &b.stmts).all(|stmt| {
                let Statement::Assign(Place::Local(d), rv) = stmt else { return true; };
                if *d != dest { return true; }
                defined = true;
                match rv {
                    Rvalue::Use(Operand::Const(Const::Null)) => true,
                    Rvalue::Use(Operand::Copy(place)) | Rvalue::Cast(Operand::Copy(place), ..)
                    | Rvalue::UnionField { base: Operand::Copy(place), .. } => base_ok(f, layouts, place, &roots),
                    Rvalue::Call { callee, args } if callee.borrowed_result => args.first()
                        .is_some_and(|op| matches!(op, Operand::Copy(place) if base_ok(f, layouts, place, &roots))),
                    _ => false,
                }
            });
            if defined && valid {
                changed |= roots.insert(dest);
            }
        }
        if !changed {
            return roots;
        }
    }
}

fn direct_aliases(f: &MirFunction, mut roots: IndexSet<Local>) -> IndexSet<Local> {
    for local in 0..f.locals.len() {
        let dest = Local(local as u32);
        let assignments: Vec<_> = f
            .blocks
            .iter()
            .flat_map(|b| &b.stmts)
            .filter_map(|stmt| match stmt {
                Statement::Assign(Place::Local(d), rv) if *d == dest => Some(rv),
                _ => None,
            })
            .collect();
        if !assignments.is_empty()
            && assignments
                .iter()
                .all(|rv| matches!(rv, Rvalue::Use(Operand::Const(Const::Null))))
        {
            roots.insert(dest);
        }
    }
    loop {
        let mut changed = false;
        for stmt in f.blocks.iter().flat_map(|b| &b.stmts) {
            if let Statement::Assign(Place::Local(dest), Rvalue::Use(Operand::Copy(Place::Local(src)))) = stmt
                && roots.contains(src)
                && f.blocks.iter().flat_map(|b| &b.stmts).all(|stmt| {
                    !matches!(stmt, Statement::Assign(Place::Local(d), rv) if d == dest
                        && !matches!(rv, Rvalue::Use(Operand::Copy(Place::Local(s))) if roots.contains(s))
                        && !matches!(rv, Rvalue::Use(Operand::Const(Const::Null))))
                }) {
                changed |= roots.insert(*dest);
            }
        }
        if !changed {
            return roots;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::effects::ModRefTable;
    use crate::build::FunctionBuilder;
    use crate::passes::rc::RcInsertion;
    use crate::{Callee, Mir};
    use dream_types::DefId;

    #[test]
    fn slot_getter_returns_zero_and_escaping_caller_acquires_one() {
        for escapes in [false, true] {
            let mut i = TypeInterner::new();
            let array = i.array(i.string());
            let mut getter = FunctionBuilder::new("get", i.string());
            getter.set_def(DefId::root(501), vec![]);
            let receiver = getter.new_param(array, None);
            let value = getter.new_local(i.string(), None);
            getter.assign(
                Place::Local(value),
                Rvalue::Use(Operand::Copy(Place::index(
                    receiver,
                    Operand::Const(Const::Int(0)),
                ))),
            );
            getter.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(value)))));
            let mut getter = getter.finish();
            getter.borrowed_result = true;
            let mut caller =
                FunctionBuilder::new("caller", if escapes { i.string() } else { i.int() });
            caller.set_def(DefId::root(502), vec![]);
            let receiver = caller.new_param(array, None);
            let value = caller.new_local(i.string(), None);
            caller.assign(
                Place::Local(value),
                Rvalue::Call {
                    callee: Callee {
                        borrowed_result: true,
                        def: getter.def,
                        args: vec![],
                        ret: i.string(),
                        take_params: vec![false],
                    },
                    args: vec![Operand::Copy(Place::Local(receiver))],
                },
            );
            if escapes {
                caller.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(value)))));
            } else {
                let len = caller.new_local(i.int(), None);
                caller.assign(
                    Place::Local(len),
                    Rvalue::StrLen(Operand::Copy(Place::Local(value))),
                );
                caller.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(len)))));
            }
            let mut mir = Mir {
                functions: vec![getter, caller.finish()],
                ..Mir::default()
            };
            let effects = ModRefTable::compute(&mir, &i);
            for f in &mut mir.functions {
                RcInsertion::run_with_layouts(f, &i, &mir.layouts, &IndexSet::new(), &effects);
            }
            let retains = |f: &MirFunction| {
                f.blocks
                    .iter()
                    .flat_map(|b| &b.stmts)
                    .filter(|s| matches!(s, Statement::Retain(_)))
                    .count()
            };
            assert_eq!(retains(&mir.functions[0]), 0);
            assert_eq!(retains(&mir.functions[1]), usize::from(escapes));
            crate::verify::assert_inserted_tokens(&mir, &i);
            use crate::passes::{Inliner, ModulePass};
            assert!(Inliner.run(&mut mir, &i));
            crate::verify::assert_inserted_tokens(&mir, &i);
        }
    }

    #[test]
    #[should_panic(expected = "must return a receiver slot")]
    fn rejects_fresh_return() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("invalid", i.string());
        b.new_param(i.string(), None);
        b.terminate(Terminator::Return(Some(Operand::Const(Const::Str(
            "fresh".into(),
        )))));
        let mut f = b.finish();
        f.borrowed_result = true;
        prepare(&mut f, &i, &LayoutTable::default(), &ModRefTable::default());
    }
}

//! Forward fields of nonescaping borrowed values without changing their ownership.

use super::global_prop::operand_eq;
use crate::{Local, MirFunction, Operand, Place, Rvalue, Statement};
use dream_types::{DefId, TypeInterner};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Slot {
    Local(Local),
    Field(Local, usize),
}
type Facts = BTreeMap<Slot, Operand>;

pub(super) fn run(f: &mut MirFunction, interner: &TypeInterner, panics: &BTreeSet<DefId>) -> bool {
    if f.is_async {
        return false;
    }
    let borrowed: Vec<_> = f
        .locals
        .iter()
        .enumerate()
        .map(|(i, l)| {
            l.borrows_refs
                && !l.is_ref
                && !f.params.contains(&Local(i as u32))
                && interner.is_value_type(l.ty)
        })
        .collect();
    if !borrowed.contains(&true) {
        return false;
    }
    let mut changed = false;
    for block in &mut f.blocks {
        if let Some(index) = block.stmts.iter().position(|s| matches!(s,
            Statement::Call { callee, .. } | Statement::Assign(_, Rvalue::Call { callee, .. }) if panics.contains(&callee.def)))
        {
            // Panic hooks run, then the intrinsic terminates; its synthetic continuation
            // cannot contribute incoming facts or a back edge to a scan loop.
            block.stmts.truncate(index + 1);
            block.terminator = crate::Terminator::Unreachable;
            changed = true;
        }
    }
    let mut analyses = super::FunctionAnalyses::default();
    let preds = analyses.predecessors(f);
    let order = analyses.reverse_postorder(f);
    let mut exit = vec![Facts::new(); f.blocks.len()];
    loop {
        let mut changed = false;
        for &b in order.iter() {
            let mut facts = incoming(b, f.entry, &preds, &exit);
            for s in &f.block(b).stmts {
                step(s, &borrowed, &mut facts);
            }
            if !facts_eq(&exit[b.0 as usize], &facts) {
                exit[b.0 as usize] = facts;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    for &b in order.iter() {
        let mut facts = incoming(b, f.entry, &preds, &exit);
        for s in &mut f.block_mut(b).stmts {
            if let Statement::Assign(_, Rvalue::Use(Operand::Copy(Place::Field { base, field }))) =
                s
                && let Some(value) = facts.get(&Slot::Field(*base, *field)).cloned()
                && let Statement::Assign(_, rv) = s
            {
                *rv = Rvalue::Use(value);
                changed = true;
            }
            step(s, &borrowed, &mut facts);
        }
    }
    changed
}

fn facts_eq(a: &Facts, b: &Facts) -> bool {
    a.len() == b.len()
        && a.iter()
            .all(|(k, v)| b.get(k).is_some_and(|w| operand_eq(v, w)))
}

fn incoming(
    b: crate::BlockId,
    entry: crate::BlockId,
    preds: &[Vec<crate::BlockId>],
    exit: &[Facts],
) -> Facts {
    if b == entry {
        return Facts::new();
    }
    let mut sources = preds[b.0 as usize].iter();
    let Some(first) = sources.next() else {
        return Facts::new();
    };
    let mut result = exit[first.0 as usize].clone();
    for p in sources {
        result.retain(|k, v| exit[p.0 as usize].get(k).is_some_and(|w| operand_eq(v, w)));
    }
    result
}

fn resolve(v: &Operand, facts: &Facts) -> Option<Operand> {
    match v {
        Operand::Const(_) if operand_eq(v, v) => Some(v.clone()),
        Operand::Copy(Place::Local(l)) => Some(facts.get(&Slot::Local(*l)).unwrap_or(v).clone()),
        Operand::Copy(Place::Field { base, field }) => {
            facts.get(&Slot::Field(*base, *field)).cloned()
        }
        _ => None,
    }
}

fn step(s: &Statement, borrowed: &[bool], facts: &mut Facts) {
    match s {
        Statement::Assign(Place::Local(d), rv) => {
            let value = if let Rvalue::Use(v) = rv {
                resolve(v, facts)
            } else {
                None
            };
            let copied = match rv {
                Rvalue::Use(Operand::Copy(Place::Local(src)))
                    if borrowed[d.0 as usize] && borrowed[src.0 as usize] =>
                {
                    facts
                        .iter()
                        .filter_map(|(slot, v)| match slot {
                            Slot::Field(base, field) if base == src => {
                                Some((Slot::Field(*d, *field), v.clone()))
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                }
                _ => vec![],
            };
            facts.retain(|slot, v| {
                let base = match slot {
                    Slot::Local(l) | Slot::Field(l, _) => l,
                };
                base != d && !matches!(v, Operand::Copy(Place::Local(l)) if l == d)
            });
            facts.extend(copied);
            if let Some(v) = value
                && !borrowed[d.0 as usize]
                && !matches!(&v, Operand::Copy(Place::Local(l)) if l == d)
            {
                facts.insert(Slot::Local(*d), v);
            }
            let quiet = super::str_cursor::rvalue_is_scan(rv)
                || borrowed[d.0 as usize] && matches!(rv, Rvalue::New { ctor: None, .. });
            if !quiet {
                facts.clear();
            }
        }
        Statement::Assign(Place::Field { base, field }, Rvalue::Use(v))
            if borrowed[base.0 as usize] =>
        {
            let value = resolve(v, facts);
            facts.remove(&Slot::Field(*base, *field));
            if let Some(v) = value {
                facts.insert(Slot::Field(*base, *field), v);
            }
        }
        Statement::Nop | Statement::DebugLine(_) | Statement::SourceLine(_) => {}
        // Unknown effects may mutate a source or a borrowed aggregate through an alias.
        _ => facts.clear(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::FunctionBuilder;
    use crate::{Const, Terminator};
    use dream_types::{DefKind, TypeCtx};

    fn fixture(overwrite: bool, reference: bool) -> (MirFunction, TypeInterner, Local, Local) {
        let mut ctx = TypeCtx::new();
        let def = ctx.register(DefKind::Struct, "View", vec![]);
        ctx.interner.mark_value_def(def);
        let ty = ctx.interner.struct_ty(def, vec![]);
        let mut b = FunctionBuilder::new("read", ctx.interner.string());
        let source = b.new_param(ctx.interner.string(), None);
        let view = b.new_local(ty, None);
        let copied = b.new_local(ty, None);
        let temp = b.new_local(ctx.interner.string(), None);
        let result = b.new_local(ctx.interner.string(), None);
        b.assign(
            Place::Field {
                base: view,
                field: 0,
            },
            Rvalue::Use(Operand::Copy(Place::Local(source))),
        );
        b.assign(
            Place::Local(temp),
            Rvalue::Use(Operand::Copy(Place::Field {
                base: view,
                field: 0,
            })),
        );
        b.assign(
            Place::Field {
                base: copied,
                field: 0,
            },
            Rvalue::Use(Operand::Copy(Place::Local(temp))),
        );
        b.assign(Place::Local(temp), Rvalue::Use(Operand::Const(Const::Null)));
        let next = b.new_block();
        b.terminate(Terminator::Goto(next));
        b.switch_to(next);
        if overwrite {
            b.assign(
                Place::Local(source),
                Rvalue::Use(Operand::Const(Const::Str("changed".into()))),
            );
        }
        b.assign(
            Place::Local(result),
            Rvalue::Use(Operand::Copy(Place::Field {
                base: copied,
                field: 0,
            })),
        );
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(
            result,
        )))));
        let mut f = b.finish();
        for l in [view, copied] {
            f.locals[l.0 as usize].borrows_refs = true;
            f.locals[l.0 as usize].is_ref = reference;
        }
        (f, ctx.interner, source, result)
    }

    #[test]
    fn copies_preserve_the_source_after_temporary_cleanup() {
        let (mut f, i, source, result) = fixture(false, false);
        run(&mut f, &i, &BTreeSet::new());
        assert!(f.blocks.iter().flat_map(|b| &b.stmts).any(|s| matches!(s,
            Statement::Assign(Place::Local(d), Rvalue::Use(Operand::Copy(Place::Local(l)))) if *d == result && *l == source)));
    }

    #[test]
    fn reassignment_invalidates_source_facts() {
        let (mut f, i, _, result) = fixture(true, false);
        run(&mut f, &i, &BTreeSet::new());
        assert!(f.blocks.iter().flat_map(|b| &b.stmts).any(|s| matches!(s,
            Statement::Assign(Place::Local(d), Rvalue::Use(Operand::Copy(Place::Field { .. }))) if *d == result)));
    }

    #[test]
    fn reference_aliases_are_not_forwarded() {
        let (mut f, i, _, result) = fixture(false, true);
        run(&mut f, &i, &BTreeSet::new());
        assert!(f.blocks.iter().flat_map(|b| &b.stmts).any(|s| matches!(s,
            Statement::Assign(Place::Local(d), Rvalue::Use(Operand::Copy(Place::Field { .. }))) if *d == result)));
    }

    #[test]
    fn disagreeing_paths_and_unknown_effects_discard_fields() {
        let field = Slot::Field(Local(0), 0);
        let first = Facts::from([(field, Operand::Const(Const::Str("first".into())))]);
        let second = Facts::from([(field, Operand::Const(Const::Str("second".into())))]);
        let preds = vec![vec![], vec![], vec![crate::BlockId(0), crate::BlockId(1)]];
        assert!(
            incoming(
                crate::BlockId(2),
                crate::BlockId(0),
                &preds,
                &[first.clone(), second]
            )
            .is_empty()
        );
        let mut facts = first;
        step(
            &Statement::Release(Operand::Copy(Place::Local(Local(1)))),
            &[true, false],
            &mut facts,
        );
        assert!(facts.is_empty());
    }

    #[test]
    fn panic_has_no_returning_continuation() {
        let (mut f, i, _, _) = fixture(false, false);
        let panic = DefId::root(900);
        f.blocks[0].stmts.push(Statement::Call {
            callee: crate::Callee {
                def: panic,
                args: vec![],
                ret: i.void(),
                take_params: vec![],
            },
            args: vec![],
        });
        run(&mut f, &i, &BTreeSet::from([panic]));
        assert!(matches!(f.blocks[0].terminator, Terminator::Unreachable));
    }

    #[test]
    fn loop_views_allow_one_payload_load_in_the_preheader() {
        use super::super::MirPass;
        let mut ctx = TypeCtx::new();
        let def = ctx.register(DefKind::Struct, "View", vec![]);
        ctx.interner.mark_value_def(def);
        let ty = ctx.interner.struct_ty(def, vec![]);
        let mut b = FunctionBuilder::new("scan", ctx.interner.void());
        let source = b.new_param(ctx.interner.string(), None);
        let condition = b.new_param(ctx.interner.bool(), None);
        let view = b.new_local(ty, None);
        let read = b.new_local(ctx.interner.string(), None);
        let unit = b.new_local(ctx.interner.char(), None);
        let header = b.new_block();
        let body = b.new_block();
        let done = b.new_block();
        b.terminate(Terminator::Goto(header));
        b.switch_to(header);
        b.terminate(Terminator::If {
            cond: Operand::Copy(Place::Local(condition)),
            then_blk: body,
            else_blk: done,
        });
        b.switch_to(body);
        b.assign(
            Place::Local(view),
            Rvalue::New {
                def,
                ty,
                ctor: None,
                args: vec![],
            },
        );
        b.assign(
            Place::Field {
                base: view,
                field: 0,
            },
            Rvalue::Use(Operand::Copy(Place::Local(source))),
        );
        b.assign(
            Place::Local(read),
            Rvalue::Use(Operand::Copy(Place::Field {
                base: view,
                field: 0,
            })),
        );
        b.assign(
            Place::Local(unit),
            Rvalue::CharAt(
                Operand::Copy(Place::Local(read)),
                Operand::Const(Const::Int(0)),
                true,
            ),
        );
        b.push(Statement::ValueKill(view));
        b.terminate(Terminator::Goto(header));
        b.switch_to(done);
        b.terminate(Terminator::Return(None));
        let mut f = b.finish();
        f.locals[view.0 as usize].borrows_refs = true;
        run(&mut f, &ctx.interner, &BTreeSet::new());
        // Copy propagation exposes the forwarded local to the cursor pass.
        super::super::GlobalProp.transform(
            &mut f,
            &ctx.interner,
            &Default::default(),
            &mut Default::default(),
        );
        assert!(super::super::StrCursor.transform(
            &mut f,
            &ctx.interner,
            &Default::default(),
            &mut Default::default()
        ));
        assert!(
            f.block(body)
                .stmts
                .iter()
                .any(|s| matches!(s, Statement::Assign(_, Rvalue::LoadU16(..))))
        );
        assert!(
            !f.block(body)
                .stmts
                .iter()
                .any(|s| matches!(s, Statement::Assign(_, Rvalue::StrBytes(_))))
        );
    }
}

//! Expose value constructors as ordinary calls so the CFG inliner preserves validation and
//! makes their field stores visible to value borrowing. Heap constructors retain their sink ABI.

use crate::{Callee, MirFunction, Operand, Place, Rvalue, Statement};
use dream_types::{DefId, TyKind, TypeId, TypeInterner};
use std::collections::BTreeMap;

type Signatures = BTreeMap<(DefId, Vec<TypeId>), Vec<TypeId>>;

pub(super) fn expand(
    f: &mut MirFunction,
    interner: &TypeInterner,
    signatures: &Signatures,
) -> bool {
    let mut changed = false;
    for block in &mut f.blocks {
        let mut statements = Vec::with_capacity(block.stmts.len());
        for statement in block.stmts.drain(..) {
            let Statement::Assign(
                Place::Local(dest),
                Rvalue::New {
                    def,
                    ty,
                    ctor: Some(ctor),
                    args,
                },
            ) = &statement
            else {
                statements.push(statement);
                continue;
            };
            if !matches!(interner.kind(*ty), TyKind::Struct(..)) || !interner.is_value_type(*ty) {
                statements.push(statement);
                continue;
            }
            let Some(parameters) = signatures.get(&(ctor.def, Vec::new())) else {
                statements.push(statement);
                continue;
            };
            if parameters.len() != args.len() + 1 {
                statements.push(statement);
                continue;
            }
            // An aliased/rebound destination can still be observed by an argument while the
            // constructor runs. Keep that combined New until a separate alias proof exists.
            if f.locals[dest.0 as usize].is_ref
                || args
                    .iter()
                    .any(|argument| super::operand_mentions(argument, *dest))
            {
                statements.push(statement);
                continue;
            }
            let (dest, def, ty, ctor, mut args) = (*dest, *def, *ty, ctor.clone(), args.clone());
            statements.push(Statement::Assign(
                Place::Local(dest),
                Rvalue::New {
                    def,
                    ty,
                    ctor: None,
                    args: Vec::new(),
                },
            ));
            args.insert(0, Operand::Copy(Place::Local(dest)));
            let mut take_params = vec![false];
            take_params.extend(ctor.take_params);
            statements.push(Statement::Call {
                callee: Callee {
                    def: ctor.def,
                    args: Vec::new(),
                    ret: interner.void(),
                    take_params,
                },
                args,
            });
            changed = true;
        }
        block.stmts = statements;
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::FunctionBuilder;
    use crate::{Const, NewCtor, Terminator};
    use dream_types::{DefKind, TypeCtx};

    #[test]
    fn only_fresh_value_construction_exposes_the_validation_call() {
        for (value_type, rebind, aliased) in [
            (true, false, false),
            (false, false, false),
            (true, true, false),
            (true, false, true),
        ] {
            let mut ctx = TypeCtx::new();
            let def = ctx.register(DefKind::Struct, "View", vec![]);
            if value_type {
                ctx.interner.mark_value_def(def);
            }
            let ty = ctx.interner.struct_ty(def, vec![]);
            let ctor_def = ctx.register(DefKind::Function, "constructor", vec![]);
            let mut b = FunctionBuilder::new("caller", ctx.interner.void());
            let dest = if aliased {
                b.new_ref_param(ty, Some("value".into()))
            } else {
                b.new_local(ty, Some("value".into()))
            };
            b.assign(
                Place::Local(dest),
                Rvalue::New {
                    def,
                    ty,
                    ctor: Some(NewCtor {
                        field_init: false,
                        def: ctor_def,
                        take_params: vec![false],
                    }),
                    args: vec![if rebind {
                        Operand::Copy(Place::Field {
                            base: dest,
                            field: 0,
                        })
                    } else {
                        Operand::Const(Const::Int(3))
                    }],
                },
            );
            b.terminate(Terminator::Return(None));
            let mut f = b.finish();
            let signatures = BTreeMap::from([((ctor_def, vec![]), vec![ty, ctx.interner.int()])]);
            let expanded = expand(&mut f, &ctx.interner, &signatures);
            assert_eq!(expanded, value_type && !rebind && !aliased);
            if expanded {
                assert!(matches!(
                    &f.blocks[0].stmts[0],
                    Statement::Assign(_, Rvalue::New { ctor: None, .. })
                ));
                assert!(
                    matches!(&f.blocks[0].stmts[1], Statement::Call { callee, args }
                    if callee.def == ctor_def && callee.take_params == [false, false]
                        && matches!(&args[0], Operand::Copy(Place::Local(l)) if *l == dest))
                );
            } else {
                assert!(matches!(
                    &f.blocks[0].stmts[0],
                    Statement::Assign(_, Rvalue::New { ctor: Some(_), .. })
                ));
            }
        }
    }
}

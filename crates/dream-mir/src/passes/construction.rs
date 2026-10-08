use crate::{Mir, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::TypeInterner;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn run(mir: &mut Mir, interner: &TypeInterner) {
    let safe: BTreeMap<_, _> = mir
        .functions
        .iter()
        .map(|f| (f.def, simple_initializer(f, mir, interner)))
        .collect();
    let zero_initializers: BTreeSet<_> = mir
        .functions
        .iter()
        .filter_map(|f| {
            let this = *f.params.first()?;
            super::construction_builders::null_initializer(mir, interner, f.def, f.local_ty(this))
                .then_some(f.def)
        })
        .collect();
    for f in &mut mir.functions {
        for statement in f.blocks.iter_mut().flat_map(|b| &mut b.stmts) {
            if let Statement::Assign(_, Rvalue::New { ctor, args, .. }) = statement
                && let Some(initializer) = ctor
            {
                if args.is_empty() && zero_initializers.contains(&initializer.def) {
                    // Allocation already zeroes the payload; empty niche variants create
                    // no observer and require no constructor call or ownership boundary.
                    *ctor = None;
                } else {
                    initializer.batched = safe.get(&initializer.def).copied().unwrap_or(false);
                }
            }
        }
    }
    super::construction_builders::run(mir, interner);
}

fn simple_initializer(f: &MirFunction, mir: &Mir, interner: &TypeInterner) -> bool {
    if f.is_async || f.blocks.len() != 1 || f.params.is_empty() || !f.instance.is_empty() {
        return false;
    }
    let this = f.params[0];
    if !crate::ownership::cycle_capable(&mir.layouts, interner, f.local_ty(this)) {
        return false;
    }
    let Some(layout) = mir.layouts.get(f.local_ty(this)) else {
        return false;
    };
    if layout.has_destructor()
        || layout
            .fields
            .iter()
            .any(|field| field.is_weak || field.is_unowned)
    {
        return false;
    }
    let block = &f.blocks[0];
    if !matches!(block.terminator, Terminator::Return(None)) {
        return false;
    }
    let mut fields = BTreeSet::new();
    let mut stored = BTreeSet::new();
    for statement in &block.stmts {
        match statement {
            Statement::Assign(Place::Field { base, field }, value) if *base == this => {
                if !fields.insert(*field) {
                    return false;
                }
                let Some(field) = layout.fields.get(*field) else {
                    return false;
                };
                if interner.is_value_type(field.ty) {
                    return false;
                }
                match value {
                    Rvalue::Use(Operand::Const(_)) => {}
                    Rvalue::Use(Operand::Copy(Place::Local(param)))
                    | Rvalue::Move {
                        src: param,
                        cast: None,
                    } if *param != this && f.params.contains(param) => {
                        stored.insert(*param);
                    }
                    _ => return false,
                }
            }
            Statement::Retain(Operand::Copy(Place::Local(param)))
                if *param != this && f.params.contains(param) => {}
            // A stored argument still has a field owner: its cleanup cannot run user code.
            Statement::Release(Operand::Copy(Place::Local(param))) if stored.contains(param) => {}
            Statement::Assign(
                Place::Local(param),
                Rvalue::Use(Operand::Const(crate::Const::Null)),
            ) if stored.contains(param) => {}
            Statement::SourceLine(_) => {}
            _ => return false,
        }
    }
    !fields.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::FunctionBuilder;
    use dream_hir::TypeLayout;
    use dream_types::{DefKind, TypeCtx};

    #[test]
    fn only_fresh_field_initialization_can_hold_the_runtime_gate() {
        for variant in 0..7 {
            let mut ctx = TypeCtx::new();
            let def = ctx.register(DefKind::Struct, "Node", vec![]);
            let ctor = ctx.register(DefKind::Function, "constructor", vec![]);
            let ty = ctx.interner.struct_ty(def, vec![]);
            let mut layout = TypeLayout::from_fields(
                &ctx.interner,
                "Node",
                [("child".into(), ty, variant == 3, false)],
            );
            if variant == 4 {
                layout.destructor = Some(ctor);
            }
            let mut mir = Mir::default();
            mir.layouts.insert(ty, layout);
            let mut b = FunctionBuilder::new("constructor", ctx.interner.void());
            let this = b.new_param(ty, Some("this".into()));
            let arg = b.new_param(ty, None);
            if variant == 5 {
                b.push(Statement::Release(Operand::Copy(Place::Local(arg))));
            }
            b.push(Statement::SourceLine(1));
            b.assign(
                Place::Field {
                    base: this,
                    field: 0,
                },
                if variant == 6 {
                    Rvalue::Move {
                        src: arg,
                        cast: None,
                    }
                } else {
                    Rvalue::Use(Operand::Copy(Place::Local(arg)))
                },
            );
            if variant == 6 {
                b.assign(
                    Place::Local(arg),
                    Rvalue::Use(Operand::Const(crate::Const::Null)),
                );
            }
            if variant == 1 {
                b.push(Statement::Print {
                    arg: Operand::Const(crate::Const::Int(1)),
                    ty: ctx.interner.int(),
                    newline: true,
                });
            }
            if variant == 2 {
                b.assign(
                    Place::Field {
                        base: this,
                        field: 0,
                    },
                    Rvalue::Use(Operand::Copy(Place::Local(arg))),
                );
            }
            b.terminate(Terminator::Return(None));
            assert_eq!(
                simple_initializer(&b.finish(), &mir, &ctx.interner),
                variant == 0 || variant == 6
            );
        }
    }
}

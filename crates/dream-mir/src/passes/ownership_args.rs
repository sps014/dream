//! Taken projections become typed locals before RC insertion. This makes every transferred
//! token explicit and also protects globals: a callee cannot adopt their resident count.

use crate::{Const, Local, LocalDecl, Mir, MirFunction, Operand, Place, Rvalue, Statement};
use dream_hir::LayoutTable;
use dream_types::{TyKind, TypeId, TypeInterner};
use std::collections::BTreeMap;

pub(crate) const STAGE: &str = "ownership-args";

pub(crate) fn run(mir: &mut Mir, interner: &TypeInterner) {
    let globals: BTreeMap<_, _> = mir.globals.iter().map(|g| (g.id, g.ty)).collect();
    for f in mir.functions.iter_mut().chain(&mut mir.polls) {
        normalize(f, interner, &mir.layouts, &globals);
    }
}

fn temp(f: &mut MirFunction, ty: TypeId) -> Local {
    let l = Local(f.locals.len() as u32);
    f.locals.push(LocalDecl {
        ty,
        name: None,
        is_ref: false,
        is_take: false,
        is_cursor: false,
        manual_drop: false,
    });
    l
}

fn normalize(
    f: &mut MirFunction,
    interner: &TypeInterner,
    layouts: &LayoutTable,
    globals: &BTreeMap<crate::Global, TypeId>,
) {
    for bi in 0..f.blocks.len() {
        let old = std::mem::take(&mut f.blocks[bi].stmts);
        let mut out = Vec::with_capacity(old.len());
        for mut stmt in old {
            let taking = match &mut stmt {
                Statement::Call { callee, args }
                | Statement::Assign(_, Rvalue::Call { callee, args }) => {
                    Some((&callee.take_params, args))
                }
                Statement::Assign(
                    _,
                    Rvalue::New {
                        ctor: Some(ctor),
                        args,
                        ..
                    },
                ) => Some((&ctor.take_params, args)),
                _ => None,
            };
            if let Some((takes, args)) = taking {
                for (index, arg) in args.iter_mut().enumerate() {
                    if !takes.get(index).copied().unwrap_or(false)
                        || matches!(arg, Operand::Copy(Place::Local(_)))
                    {
                        continue;
                    }
                    let ty = operand_type(f, interner, layouts, globals, arg);
                    let local = temp(f, ty);
                    out.push(Statement::Assign(
                        Place::Local(local),
                        Rvalue::Use(arg.clone()),
                    ));
                    *arg = Operand::Copy(Place::Local(local));
                }
            }
            out.push(stmt);
        }
        f.blocks[bi].stmts = out;
    }
}

fn operand_type(
    f: &MirFunction,
    i: &TypeInterner,
    layouts: &LayoutTable,
    globals: &BTreeMap<crate::Global, TypeId>,
    op: &Operand,
) -> TypeId {
    match op {
        Operand::Copy(Place::Local(l)) => f.local_ty(*l),
        Operand::Copy(Place::Global(g)) => *globals.get(g).expect("declared global"),
        Operand::Copy(Place::Field { base, field }) => {
            layouts
                .get(f.local_ty(*base))
                .and_then(|l| l.fields.get(*field))
                .expect("validated field")
                .ty
        }
        Operand::Copy(Place::Index { base, .. }) => match i.kind(f.local_ty(*base)) {
            TyKind::Array(elem) => *elem,
            _ => crate::internal_error!("index of non-array"),
        },
        Operand::Copy(Place::Deref { elem_ty, .. }) => *elem_ty,
        Operand::Const(Const::Str(_)) => i.string(),
        Operand::Const(Const::Long(_)) => i.long(),
        Operand::Const(Const::Float(_)) => i.double(),
        Operand::Const(Const::F32(_)) => i.float(),
        Operand::Const(Const::Bool(_)) => i.bool(),
        Operand::Const(Const::Char(_)) => i.char(),
        Operand::Const(Const::Int(_) | Const::Null) => i.int(),
    }
}

use crate::{Callee, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_hir::LayoutTable;
use dream_types::{TyKind, TypeId, TypeInterner};
use indexmap::IndexSet;

pub(super) struct Call {
    pub(super) callee: Callee,
    pub(super) arguments: Vec<IndexSet<usize>>,
}

pub(super) struct Proof {
    pub(super) safe: bool,
    pub(super) parameters: Vec<bool>,
    pub(super) calls: Vec<Call>,
}

fn scalar(ty: TypeId, i: &TypeInterner) -> bool {
    matches!(i.kind(ty), TyKind::Void | TyKind::Enum(_))
        || matches!(i.kind(ty), TyKind::Prim(_)) && !i.is_rc_tracked(ty)
}

// Removing a count must not alter a user destructor or weak/unowned observation, even when
// the argument's last use is the call. Recursive layouts stay outside this finite proof.
fn transparent(
    ty: TypeId,
    i: &TypeInterner,
    layouts: &LayoutTable,
    visiting: &mut IndexSet<TypeId>,
) -> bool {
    if scalar(ty, i) || ty == i.string() {
        return true;
    }
    if !visiting.insert(ty) {
        return false;
    }
    let yes = match i.kind(ty) {
        TyKind::Array(elem) => transparent(*elem, i, layouts, visiting),
        TyKind::Struct(..) => layouts.get(ty).is_some_and(|l| {
            !l.has_destructor()
                && l.fields.iter().all(|field| {
                    !field.is_weak
                        && !field.is_unowned
                        && transparent(field.ty, i, layouts, visiting)
                })
        }),
        _ => false,
    };
    visiting.shift_remove(&ty);
    yes
}

fn roots(
    op: &Operand,
    aliases: &[IndexSet<usize>],
    f: &MirFunction,
    i: &TypeInterner,
    layouts: &LayoutTable,
) -> IndexSet<usize> {
    let (base, ty) = match op {
        Operand::Copy(Place::Local(l)) => (*l, f.local_ty(*l)),
        Operand::Copy(Place::Field { base, field }) => {
            let Some(field) = layouts
                .get(f.local_ty(*base))
                .and_then(|l| l.fields.get(*field))
            else {
                return IndexSet::new();
            };
            (*base, field.ty)
        }
        Operand::Copy(Place::Index { base, .. }) => {
            let TyKind::Array(elem) = i.kind(f.local_ty(*base)) else {
                return IndexSet::new();
            };
            (*base, *elem)
        }
        _ => return IndexSet::new(),
    };
    if scalar(ty, i) {
        IndexSet::new()
    } else {
        aliases[base.0 as usize].clone()
    }
}

fn sources(
    rv: &Rvalue,
    aliases: &[IndexSet<usize>],
    f: &MirFunction,
    i: &TypeInterner,
    layouts: &LayoutTable,
) -> IndexSet<usize> {
    let ops: Vec<_> = match rv {
        Rvalue::Use(op) | Rvalue::Cast(op, ..) => vec![op],
        Rvalue::Select {
            then_val, else_val, ..
        } => vec![then_val, else_val],
        // A managed call result may alias any argument. Scalar results carry no ownership.
        Rvalue::Call { args, .. } => args.iter().collect(),
        _ => vec![],
    };
    ops.into_iter()
        .flat_map(|op| roots(op, aliases, f, i, layouts))
        .collect()
}

fn permitted(stmt: &Statement, f: &MirFunction, i: &TypeInterner, layouts: &LayoutTable) -> bool {
    match stmt {
        Statement::Assign(place, rv) => {
            let store = match place {
                Place::Local(_) => true,
                Place::Index { base, .. } => {
                    matches!(i.kind(f.local_ty(*base)), TyKind::Array(elem) if scalar(*elem, i))
                }
                Place::Field { base, field } => layouts
                    .get(f.local_ty(*base))
                    .and_then(|l| l.fields.get(*field))
                    .is_some_and(|field| scalar(field.ty, i)),
                _ => false,
            };
            store
                && matches!(
                    rv,
                    Rvalue::Use(_)
                        | Rvalue::Zero(_)
                        | Rvalue::Binary(..)
                        | Rvalue::CheckedBinary(..)
                        | Rvalue::Unary(..)
                        | Rvalue::CheckedNeg(_)
                        | Rvalue::Cast(..)
                        | Rvalue::ArrayLen(_)
                        | Rvalue::StrLen(_)
                        | Rvalue::StrByteSize(_)
                        | Rvalue::CharAt(..)
                        | Rvalue::ByteAt(..)
                        | Rvalue::Select { .. }
                        | Rvalue::Call { .. }
                        | Rvalue::ArrayNew {
                            closure_env: false,
                            ..
                        }
                )
        }
        Statement::Call { .. } | Statement::Nop | Statement::SourceLine(_) => true,
        _ => false,
    }
}

impl Proof {
    pub(super) fn of(f: &MirFunction, i: &TypeInterner, layouts: &LayoutTable) -> Self {
        let safe = !f.is_async
            && f.locals
                .iter()
                .all(|d| !d.is_ref && transparent(d.ty, i, layouts, &mut IndexSet::new()))
            && f.blocks.iter().all(|b| {
                b.stmts.iter().all(|s| permitted(s, f, i, layouts))
                    && matches!(
                        b.terminator,
                        Terminator::Goto(_)
                            | Terminator::If { .. }
                            | Terminator::Switch { .. }
                            | Terminator::Return(_)
                            | Terminator::Unreachable
                    )
            });
        let mut parameters: Vec<_> = f
            .params
            .iter()
            .map(|p| safe && !scalar(f.local_ty(*p), i))
            .collect();
        if !safe {
            return Self {
                safe,
                parameters,
                calls: Vec::new(),
            };
        }
        let mut aliases = vec![IndexSet::new(); f.locals.len()];
        for (index, p) in f.params.iter().enumerate() {
            aliases[p.0 as usize].insert(index);
        }
        // Union over every reaching assignment handles joins, loops and backedges. Keeping killed
        // aliases is conservative: it can reject an optimization but cannot hide an escape.
        loop {
            let mut changed = false;
            for stmt in f.blocks.iter().flat_map(|b| &b.stmts) {
                if let Statement::Assign(Place::Local(dst), rv) = stmt
                    && !scalar(f.local_ty(*dst), i)
                {
                    let incoming = sources(rv, &aliases, f, i, layouts);
                    let before = aliases[dst.0 as usize].len();
                    aliases[dst.0 as usize].extend(incoming);
                    changed |= aliases[dst.0 as usize].len() != before;
                }
            }
            if !changed {
                break;
            }
        }
        let mut calls = Vec::new();
        for block in &f.blocks {
            for stmt in &block.stmts {
                if let Statement::Assign(Place::Local(dst), _) = stmt
                    && let Some(index) = f.params.iter().position(|p| p == dst)
                {
                    // Borrowed parameters have no owning slot for a replacement allocation.
                    parameters[index] = false;
                }
                let call = match stmt {
                    Statement::Call { callee, args }
                    | Statement::Assign(_, Rvalue::Call { callee, args }) => Some((callee, args)),
                    _ => None,
                };
                if let Some((callee, args)) = call {
                    calls.push(Call {
                        callee: callee.clone(),
                        arguments: args
                            .iter()
                            .map(|op| roots(op, &aliases, f, i, layouts))
                            .collect(),
                    });
                }
            }
            if let Terminator::Return(Some(op)) = &block.terminator {
                for index in roots(op, &aliases, f, i, layouts) {
                    parameters[index] = false;
                }
            }
        }
        Self {
            safe,
            parameters,
            calls,
        }
    }
}

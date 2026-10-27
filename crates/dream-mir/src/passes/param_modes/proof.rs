use crate::analysis::effects::{ModRef, ModRefTable, strong_children};
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

// A repeated layout closes an edge in the finite strong-field graph. Every node still
// checks its own observer/destructor restrictions before the DFS completes.
fn transparent(
    ty: TypeId,
    i: &TypeInterner,
    layouts: &LayoutTable,
    visiting: &mut IndexSet<TypeId>,
) -> bool {
    if scalar(ty, i) || ty == i.string() {
        return true;
    }
    if i.is_shared_type(ty) {
        return false;
    }
    if !visiting.insert(ty) {
        return true;
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
        TyKind::Tuple(fields) => fields.iter().all(|&t| transparent(t, i, layouts, visiting)),
        TyKind::Union(..) => layouts.union(ty).is_some_and(|l| {
            !l.has_destructor()
                && l.variants.iter().flat_map(|v| &v.fields).all(|field| {
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

pub(super) fn roots(
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

pub(super) fn sources(
    rv: &Rvalue,
    aliases: &[IndexSet<usize>],
    f: &MirFunction,
    i: &TypeInterner,
    layouts: &LayoutTable,
    returns: &super::aliases::ReturnSummaries,
) -> IndexSet<usize> {
    let ops: Vec<_> = match rv {
        Rvalue::Use(op) | Rvalue::Cast(op, ..) => vec![op],
        Rvalue::Select {
            then_val, else_val, ..
        } => vec![then_val, else_val],
        // Closed return summaries select the contributing arguments; missing bodies
        // preserve every possible dependency. Scalar results carry no ownership.
        Rvalue::Call { callee, args } => args
            .iter()
            .enumerate()
            .filter(|(index, _)| {
                returns
                    .get(&super::key(callee))
                    .is_none_or(|roots| roots.contains(index))
            })
            .map(|(_, op)| op)
            .collect(),
        Rvalue::New { args, .. }
        | Rvalue::UnionNew { args, .. }
        | Rvalue::ArrayLit { elems: args, .. }
        | Rvalue::Tuple { elems: args, .. } => args.iter().collect(),
        Rvalue::Concat(args) => args.iter().collect(),
        Rvalue::ConcatInt { prefix, suffix, .. } => vec![prefix, suffix],
        Rvalue::ToString(op) => vec![op],
        Rvalue::UnionField { base, .. } => vec![base],
        _ => vec![],
    };
    ops.into_iter()
        .flat_map(|op| roots(op, aliases, f, i, layouts))
        .collect()
}

fn permitted(stmt: &Statement, f: &MirFunction, i: &TypeInterner, layouts: &LayoutTable) -> bool {
    match stmt {
        Statement::Assign(place, rv) => {
            if let Rvalue::New { ty, .. } = rv
                && !transparent(*ty, i, layouts, &mut IndexSet::new())
            {
                return false;
            }
            let store = match place {
                Place::Local(_) => true,
                Place::Index { base, .. } => {
                    matches!(i.kind(f.local_ty(*base)), TyKind::Array(elem) if transparent(*elem, i, layouts, &mut IndexSet::new()))
                }
                Place::Field { base, field } => layouts
                    .get(f.local_ty(*base))
                    .and_then(|l| l.fields.get(*field))
                    .is_some_and(|field| {
                        !field.is_weak
                            && !field.is_unowned
                            && transparent(field.ty, i, layouts, &mut IndexSet::new())
                    }),
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
                        | Rvalue::New { ctor: None, .. }
                        | Rvalue::UnionNew { .. }
                        | Rvalue::UnionField { .. }
                        | Rvalue::Tuple { .. }
                        | Rvalue::ArrayLit { .. }
                        | Rvalue::Concat(_)
                        | Rvalue::ConcatInt { .. }
                        | Rvalue::ToString(_)
                        | Rvalue::HashCode(_)
                        | Rvalue::IsType(..)
                        | Rvalue::TypeName(_)
                        | Rvalue::Discriminant { .. }
                )
        }
        Statement::Print { ty, .. } => matches!(i.kind(*ty), TyKind::Prim(_)),
        Statement::Call { .. } | Statement::Nop | Statement::SourceLine(_) => true,
        _ => false,
    }
}

fn may_reach(
    ty: TypeId,
    target: TypeId,
    i: &TypeInterner,
    layouts: &LayoutTable,
    seen: &mut IndexSet<TypeId>,
) -> bool {
    if ty == target {
        return true;
    }
    if scalar(ty, i) || !seen.insert(ty) {
        return false;
    }
    strong_children(ty, i, layouts).is_none_or(|children| {
        children
            .into_iter()
            .any(|child| may_reach(child, target, i, layouts, seen))
    })
}

impl Proof {
    pub(super) fn of(
        f: &MirFunction,
        i: &TypeInterner,
        layouts: &LayoutTable,
        effects: &ModRefTable,
        returns: &super::aliases::ReturnSummaries,
    ) -> Self {
        let safe = !f.is_async
            && f.locals.iter().all(|d| !d.is_ref)
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
            .map(|p| {
                safe && !scalar(f.local_ty(*p), i)
                    && transparent(f.local_ty(*p), i, layouts, &mut IndexSet::new())
            })
            .collect();
        if !safe || !parameters.iter().any(|&yes| yes) {
            return Self {
                safe,
                parameters,
                calls: Vec::new(),
            };
        }
        let states = super::aliases::compute(f, i, layouts, returns);
        // Releasing an overwritten occupant can invalidate an argument of its type, or any
        // reference reachable through it. Missing summaries keep the owning ABI.
        let summary = effects.call_def(f.def, &f.instance);
        let cleanup_safe = !summary.may_destroy() || effects.del().is_quiet();
        if !cleanup_safe {
            parameters.fill(false);
        }
        let released: Option<Vec<TypeId>> = match &summary {
            ModRef::Top => None,
            ModRef::Known(known) if known.any_slot => None,
            ModRef::Known(known) => {
                let mut types: Vec<_> = known.slots.iter().copied().collect();
                for &(ty, field) in &known.fields {
                    if let Some(field) = layouts.get(ty).and_then(|l| l.fields.get(field as usize))
                    {
                        types.push(field.ty);
                    } else {
                        types.extend(f.params.iter().map(|p| f.local_ty(*p)));
                    }
                }
                Some(types)
            }
        };
        for (index, p) in f.params.iter().enumerate() {
            if released.as_ref().is_none_or(|types| {
                types
                    .iter()
                    .any(|&ty| may_reach(ty, f.local_ty(*p), i, layouts, &mut IndexSet::new()))
            }) {
                parameters[index] = false;
            }
        }
        let mut calls = Vec::new();
        for (bi, block) in f.blocks.iter().enumerate() {
            let mut aliases = states[bi].clone();
            for stmt in &block.stmts {
                if let Statement::Assign(Place::Local(dst), _) = stmt
                    && let Some(index) = f.params.iter().position(|p| p == dst)
                {
                    // Borrowed parameters have no owning slot for a replacement allocation.
                    parameters[index] = false;
                }
                if let Statement::Assign(Place::Field { .. } | Place::Index { .. }, rv) = stmt {
                    for index in sources(rv, &aliases, f, i, layouts, returns) {
                        parameters[index] = false;
                    }
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
                if let Statement::Assign(Place::Local(dst), rv) = stmt {
                    aliases[dst.0 as usize] = sources(rv, &aliases, f, i, layouts, returns);
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

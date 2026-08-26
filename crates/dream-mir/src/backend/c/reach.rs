//! Reachability scan for the object-protocol (`to_string` / `hash_code`) emitters.
//!
//! The backend otherwise emits `{Type}_to_string` / `{Type}_hash_code` for every
//! struct, union, and array shape in the module. Most programs never stringify
//! most of their types, so this pass walks the MIR and records which protocol
//! functions are actually referenced (directly or via `print`), plus whether any
//! reference is *dynamic* — resolved through `dream_object_to_string` /
//! `dream_object_hash_code`, which can reach every tagged type and therefore
//! forces full emission.
//!
//! Set entries are the TypeId the emitter keys on: struct/union layout ids, and
//! the *element* id for `array_to_string_t{N}`.

use super::ctx::Cx;
use crate::{Operand, Rvalue, Statement};
use dream_types::{PrimTy, TyKind, TypeId};
use std::collections::BTreeSet;

#[derive(Debug, Default)]
pub(super) struct ProtocolReach {
    /// Struct/union ids whose `{Type}_to_string`, and element ids whose
    /// `array_to_string_t{N}`, are referenced from emitted code.
    pub to_string: BTreeSet<TypeId>,
    /// Struct/union ids whose `{Type}_hash_code` is referenced.
    pub hash_code: BTreeSet<TypeId>,
    /// A dynamic (layout-less) to_string/hash_code/print was seen: emit routers,
    /// which in turn need every tagged type's protocol functions.
    pub dynamic: bool,
}

impl ProtocolReach {
    pub(super) fn needs_to_string(&self, key: TypeId) -> bool {
        self.dynamic || self.to_string.contains(&key)
    }

    pub(super) fn needs_hash_code(&self, ty: TypeId) -> bool {
        self.dynamic || self.hash_code.contains(&ty)
    }
}

pub(super) fn compute(cx: &Cx<'_>) -> ProtocolReach {
    let mut reach = ProtocolReach::default();
    let mut funcs: Vec<&crate::MirFunction> = cx.mir.functions.iter().collect();
    let lowered: Vec<crate::MirFunction> = cx
        .mir
        .functions
        .iter()
        .filter(|f| f.is_async)
        .filter_map(|f| {
            f.hir_fn
                .as_ref()
                .map(|hir| crate::lower::lower_async_poll_body(hir, cx.interner, &cx.mir.layouts))
        })
        .collect();
    funcs.extend(lowered.iter());
    for f in &funcs {
        for block in &f.blocks {
            for stmt in &block.stmts {
                match stmt {
                    Statement::Assign(_, rv) => scan_rvalue(cx, f, rv, &mut reach),
                    // `print(x)` resolves through `to_string_fn(ty)` for non-primitives.
                    Statement::Print { ty, .. } => classify(cx, *ty, &mut reach),
                    _ => {}
                }
            }
        }
    }
    close_over_layouts(cx, &mut reach.to_string);
    close_over_layouts(cx, &mut reach.hash_code);
    reach
}

fn scan_rvalue(cx: &Cx<'_>, f: &crate::MirFunction, rv: &Rvalue, reach: &mut ProtocolReach) {
    match rv {
        Rvalue::ToString(o) => classify(cx, op_ty(cx, f, o), reach),
        Rvalue::HashCode(o) => {
            let ty = op_ty(cx, f, o);
            if has_local_hash(cx, ty) {
                reach.hash_code.insert(ty);
            } else if !matches!(cx.interner.kind(ty), TyKind::Prim(PrimTy::String)) {
                // Falls through to `dream_object_hash_code`.
                reach.dynamic = true;
            }
        }
        _ => {}
    }
}

fn classify(cx: &Cx<'_>, ty: TypeId, reach: &mut ProtocolReach) {
    match cx.interner.kind(ty) {
        TyKind::Array(e) => {
            reach.to_string.insert(*e);
        }
        TyKind::Enum(_) => {}
        _ if cx.nstruct(ty).is_some() || cx.nunion(ty).is_some() => {
            reach.to_string.insert(ty);
        }
        // Layout-less (class objects / interfaces / unknown): dynamic router.
        _ if !matches!(cx.interner.kind(ty), TyKind::Prim(PrimTy::String)) => {
            reach.dynamic = true;
        }
        _ => {}
    }
}

fn has_local_hash(cx: &Cx<'_>, ty: TypeId) -> bool {
    cx.nstruct(ty).is_some() || cx.nunion(ty).is_some()
}

/// A referenced layout pulls in its fields' own protocol references (a struct's
/// `to_string` calls each field's converter; an array's calls its element's).
fn close_over_layouts(cx: &Cx<'_>, set: &mut BTreeSet<TypeId>) {
    let mut work: Vec<TypeId> = set.iter().copied().collect();
    while let Some(ty) = work.pop() {
        let mut deps: Vec<TypeId> = Vec::new();
        match cx.interner.kind(ty) {
            TyKind::Void | TyKind::Prim(_) => {}
            TyKind::Array(e) => deps.push(*e),
            _ => {
                if let Some(l) = cx.nstruct(ty) {
                    deps.extend(l.fields.iter().map(|f| f.ty));
                } else if let Some(u) = cx.nunion(ty) {
                    for v in &u.variants {
                        deps.extend(v.fields.iter().map(|f| f.ty));
                    }
                }
            }
        }
        for d in deps {
            let dep_key = match cx.interner.kind(d) {
                TyKind::Prim(_) | TyKind::Void => continue,
                TyKind::Array(e) => *e,
                _ => d,
            };
            if (cx.nstruct(dep_key).is_some() || cx.nunion(dep_key).is_some()) && set.insert(dep_key)
            {
                work.push(d);
            }
        }
    }
}

fn op_ty(cx: &Cx<'_>, f: &crate::MirFunction, o: &Operand) -> TypeId {
    match o {
        Operand::Copy(crate::Place::Local(l)) => f.local_ty(*l),
        Operand::Copy(crate::Place::Global(g)) => cx
            .mir
            .globals
            .iter()
            .find(|global| global.id == *g)
            .map(|global| global.ty)
            .unwrap_or_else(|| cx.interner.int()),
        Operand::Copy(crate::Place::Field { base, field }) => cx
            .nstruct(f.local_ty(*base))
            .and_then(|layout| layout.fields.get(*field))
            .map(|field| field.ty)
            .unwrap_or_else(|| f.local_ty(*base)),
        Operand::Copy(crate::Place::Index { base, .. }) => {
            match cx.interner.kind(f.local_ty(*base)) {
                TyKind::Array(e) => *e,
                _ => f.local_ty(*base),
            }
        }
        Operand::Copy(crate::Place::Deref { elem_ty, .. }) => *elem_ty,
        Operand::Const(crate::Const::Str(_)) => cx.interner.string(),
        Operand::Const(crate::Const::Long(_)) => cx.interner.long(),
        Operand::Const(crate::Const::Float(_)) => cx.interner.double(),
        Operand::Const(crate::Const::F32(_)) => cx.interner.float(),
        Operand::Const(crate::Const::Bool(_)) => cx.interner.bool(),
        Operand::Const(crate::Const::Char(_)) => cx.interner.char(),
        Operand::Const(_) => cx.interner.int(),
    }
}

//! Shared object-protocol (`to_string` / `hash_code`) reachability for pruning and emission.
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

use crate::{Mir, MirFunction};
use crate::{Operand, Rvalue, Statement};
use dream_types::TypeInterner;
use dream_types::{TyKind, TypeId};
use std::collections::BTreeSet;

#[derive(Debug, Default)]
pub(crate) struct ProtocolReach {
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
    pub(crate) fn needs_to_string(&self, key: TypeId) -> bool {
        self.dynamic || self.to_string.contains(&key)
    }

    pub(crate) fn needs_hash_code(&self, ty: TypeId) -> bool {
        self.dynamic || self.hash_code.contains(&ty)
    }
}

struct Context<'a> {
    mir: &'a Mir,
    interner: &'a TypeInterner,
}

pub(crate) fn compute<'a>(
    mir: &'a Mir,
    interner: &TypeInterner,
    functions: impl Iterator<Item = &'a MirFunction>,
) -> ProtocolReach {
    let cx = &Context { mir, interner };
    let mut reach = ProtocolReach::default();
    for f in functions {
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
        if f.is_async
            && let Some(hir) = &f.hir_fn
        {
            let mut edges = crate::HirEdges::default();
            crate::hir_body_edges(&hir.body, &mut edges);
            for ty in edges.to_string {
                classify(cx, ty, &mut reach);
            }
            for ty in edges.hash_code {
                classify_hash(cx, ty, &mut reach);
            }
        }
    }
    // The entry point renders a failing `main`'s error payload, but that call site is emitted by
    // hand after this scan, so it has to be registered here or its converter is never defined.
    if let Some(err_ty) = entry_error_type(mir) {
        classify(cx, err_ty, &mut reach);
    }
    let mut dynamic = reach.dynamic;
    close_over_layouts(cx, &mut reach.to_string, &mut dynamic, false);
    close_over_layouts(cx, &mut reach.hash_code, &mut dynamic, true);
    reach.dynamic = dynamic;
    reach
}

fn scan_rvalue(cx: &Context<'_>, f: &crate::MirFunction, rv: &Rvalue, reach: &mut ProtocolReach) {
    match rv {
        Rvalue::ToString(o) => classify(cx, op_ty(cx, f, o), reach),
        Rvalue::HashCode(o) => classify_hash(cx, op_ty(cx, f, o), reach),
        _ => {}
    }
}

fn classify_hash(cx: &Context<'_>, ty: TypeId, reach: &mut ProtocolReach) {
    if has_local_hash(cx, ty) {
        reach.hash_code.insert(ty);
    } else if !matches!(cx.interner.kind(ty), TyKind::Prim(_) | TyKind::Enum(_)) {
        reach.dynamic = true;
    }
}

fn classify(cx: &Context<'_>, ty: TypeId, reach: &mut ProtocolReach) {
    match cx.interner.kind(ty) {
        TyKind::Array(e) => {
            reach.to_string.insert(*e);
        }
        TyKind::Prim(_) | TyKind::Enum(_) => {}
        _ if cx.mir.layouts.structs.contains_key(&ty)
            || cx.mir.layouts.unions.contains_key(&ty) =>
        {
            reach.to_string.insert(ty);
        }
        // Layout-less (class objects / interfaces / unknown): dynamic router.
        _ => reach.dynamic = true,
    }
}

fn has_local_hash(cx: &Context<'_>, ty: TypeId) -> bool {
    cx.mir.layouts.structs.contains_key(&ty) || cx.mir.layouts.unions.contains_key(&ty)
}

/// A referenced layout pulls in its fields' own protocol references (a struct's
/// `to_string` calls each field's converter; an array's calls its element's). A field with no
/// layout of its own converts through the router, so it makes the whole module dynamic even when
/// no MIR statement named such a type directly.
fn close_over_layouts(
    cx: &Context<'_>,
    set: &mut BTreeSet<TypeId>,
    dynamic: &mut bool,
    hash: bool,
) {
    let mut work: Vec<TypeId> = set.iter().copied().collect();
    while let Some(ty) = work.pop() {
        if cx.mir.object_methods.get(&ty).is_some_and(|methods| {
            if hash {
                methods.hash_code.is_some()
            } else {
                methods.to_string.is_some()
            }
        }) {
            continue;
        }
        let mut deps: Vec<TypeId> = Vec::new();
        match cx.interner.kind(ty) {
            TyKind::Void | TyKind::Prim(_) => {}
            TyKind::Array(_) => deps.push(ty),
            _ => {
                if let Some(l) = cx.mir.layouts.structs.get(&ty) {
                    deps.extend(l.fields.iter().map(|f| f.ty));
                } else if let Some(u) = cx.mir.layouts.unions.get(&ty) {
                    for v in &u.variants {
                        deps.extend(v.fields.iter().map(|f| f.ty));
                    }
                }
            }
        }
        for d in deps {
            let dep_key = match cx.interner.kind(d) {
                TyKind::Prim(_) | TyKind::Void => continue,
                TyKind::Array(e) => {
                    // `array_to_string_t{e}` is keyed by the element whatever its kind, including
                    // primitive and enum elements that have no converter of their own.
                    if set.insert(*e) {
                        work.push(*e);
                    }
                    *e
                }
                _ => d,
            };
            if cx.mir.layouts.structs.contains_key(&dep_key)
                || cx.mir.layouts.unions.contains_key(&dep_key)
            {
                if set.insert(dep_key) {
                    work.push(dep_key);
                }
            } else if !matches!(
                cx.interner.kind(dep_key),
                TyKind::Prim(_) | TyKind::Void | TyKind::Enum(_) | TyKind::Array(_)
            ) {
                // Mirrors the layout-less fallback in `rvalue::to_string_fn` / `hash_code_of`.
                *dynamic = true;
            }
        }
    }
}

fn op_ty(cx: &Context<'_>, f: &crate::MirFunction, o: &Operand) -> TypeId {
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
            .mir
            .layouts
            .structs
            .get(&f.local_ty(*base))
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
        Operand::Const(crate::Const::F16(_)) => cx.interner.float16(),
        Operand::Const(crate::Const::Bool(_)) => cx.interner.bool(),
        Operand::Const(crate::Const::Char(_)) => cx.interner.char(),
        Operand::Const(_) => cx.interner.int(),
    }
}

fn entry_error_type(mir: &Mir) -> Option<TypeId> {
    let main = mir
        .functions
        .iter()
        .find(|f| f.name == crate::abi::ENTRY_FN)?;
    let layout = mir.layouts.unions.get(&main.ret)?;
    let variant = layout.variants.iter().find(|v| v.name == "Err")?;
    variant.fields.first().map(|f| f.ty)
}

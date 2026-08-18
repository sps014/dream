//! Codegen support for inline value (`struct`) types.
//!
//! A value struct is stored *inline* (its bytes live directly in a shadow-stack frame slot, a
//! container field/element, or a union payload) rather than as a heap-allocated, reference-counted
//! pointer. At the WASM level a value-struct local is an `i32` holding the **address** of its
//! storage; reading such a place yields that address (never a load), and moving a value struct into
//! a new location performs a byte-wise copy plus a recursive retain of any reference fields.
//!
//! This module computes, per function, the shadow-frame layout and the ownership classification of
//! each value-struct local, and emits the per-type retain/drop glue (`$__vs_retain_<T>` /
//! `$__vs_drop_<T>`) that keeps reference fields embedded inside value structs balanced.

use super::*;
use std::collections::HashSet;

pub(crate) use crate::backend::shared::{is_simd_vector, ValueFrame, ValueLocalKind};

/// The `$__vs_retain_<T>` symbol: retains (increments) every reference reachable *by value* inside a
/// value struct after a byte-wise copy, so the copy owns its own references.
pub(crate) fn vs_retain_sym(name: &str) -> String {
    format!("$__vs_retain_{}", name)
}

/// The `$__vs_drop_<T>` symbol: runs `del()` (if any) then releases every reference reachable by
/// value inside a value struct, when an owning value goes out of scope or is overwritten.
pub(crate) fn vs_drop_sym(name: &str) -> String {
    format!("$__vs_drop_{}", name)
}

/// The set of value-struct types that require retain/drop glue: those that (transitively) embed a
/// reference field, or declare a `del()` destructor. Purely-scalar value structs need none, so their
/// copies and drops are plain byte moves with no bookkeeping.
pub(super) fn value_glue_types(mir: &crate::Mir, interner: &TypeInterner) -> HashSet<TypeId> {
    let fn_names: HashSet<&str> = mir.functions.iter().map(|f| f.name.as_str()).collect();
    let mut out = HashSet::new();
    let struct_keys: Vec<TypeId> = mir.layouts.structs.keys().copied().collect();
    for ty in struct_keys {
        if interner.is_value_type(ty) {
            needs_glue(ty, mir, interner, &fn_names, &mut out, &mut HashSet::new());
        }
    }
    // Value unions can also embed references (via a value-struct payload), so they too may need glue.
    let union_keys: Vec<TypeId> = mir.layouts.unions.keys().copied().collect();
    for ty in union_keys {
        if interner.is_value_union(ty) {
            needs_glue(ty, mir, interner, &fn_names, &mut out, &mut HashSet::new());
        }
    }
    out
}

/// Determines whether value type `ty` (a value struct or value union) needs glue, memoizing the
/// answer into `out` (the set of glue-requiring types). `visiting` guards the recursion (value-type
/// cycles are a rejected error).
fn needs_glue(
    ty: TypeId,
    mir: &crate::Mir,
    interner: &TypeInterner,
    fn_names: &HashSet<&str>,
    out: &mut HashSet<TypeId>,
    visiting: &mut HashSet<TypeId>,
) -> bool {
    if out.contains(&ty) {
        return true;
    }
    if !visiting.insert(ty) {
        return false;
    }
    // A value union needs glue when any variant payload is a reference or a glue-needing value type.
    if interner.is_value_union(ty) {
        let mut needs = false;
        if let Some(u) = mir.layouts.unions.get(&ty) {
            for v in &u.variants {
                for f in &v.fields {
                    if interner.is_rc_tracked(f.ty)
                        || (interner.is_value_type(f.ty)
                            && needs_glue(f.ty, mir, interner, fn_names, out, visiting))
                    {
                        needs = true;
                    }
                }
            }
        }
        visiting.remove(&ty);
        if needs {
            out.insert(ty);
        }
        return needs;
    }
    let Some(layout) = mir.layouts.structs.get(&ty) else {
        visiting.remove(&ty);
        return false;
    };
    let mut needs = fn_names.contains(format!("{}_del", layout.name).as_str());
    for f in &layout.fields {
        if interner.is_rc_tracked(f.ty)
            || (interner.is_value_type(f.ty)
                && needs_glue(f.ty, mir, interner, fn_names, out, visiting))
        {
            needs = true;
        }
    }
    visiting.remove(&ty);
    if needs {
        out.insert(ty);
    }
    needs
}

/// Emits the per-value-struct retain/drop glue for every type in `glue`.
pub(super) fn emit_value_glue(
    out: &mut String,
    mir: &crate::Mir,
    interner: &TypeInterner,
    glue: &HashSet<TypeId>,
) {
    let fn_names: HashSet<&str> = mir.functions.iter().map(|f| f.name.as_str()).collect();
    // Deterministic order: layout-table (struct) order, filtered to the glue set.
    for (ty, layout) in &mir.layouts.structs {
        if !glue.contains(ty) {
            continue;
        }
        // `$__vs_retain_<T>(ptr)`: retain each reference field; recurse into value fields.
        let _ = writeln!(
            out,
            "(func {} (param $ptr i32)",
            vs_retain_sym(&layout.name)
        );
        for f in &layout.fields {
            emit_field_glue(out, mir, interner, glue, f, GlueOp::Retain);
        }
        out.push_str(")\n");

        // `$__vs_drop_<T>(ptr)`: run `del()` (if any), then release each reference field / recurse.
        let _ = writeln!(out, "(func {} (param $ptr i32)", vs_drop_sym(&layout.name));
        let del = format!("{}_del", layout.name);
        if fn_names.contains(del.as_str()) {
            let _ = writeln!(out, "  (local.get $ptr) (call ${})", del);
        }
        for f in &layout.fields {
            emit_field_glue(out, mir, interner, glue, f, GlueOp::Drop);
        }
        out.push_str(")\n");
    }

    // Value-union glue is variant-aware: the discriminant at offset 0 selects which payload fields
    // are live, so each retain/drop guards its field work on `discriminant == variant`.
    for (ty, layout) in &mir.layouts.unions {
        if !glue.contains(ty) {
            continue;
        }
        for op in [GlueOp::Retain, GlueOp::Drop] {
            let sym = match op {
                GlueOp::Retain => vs_retain_sym(&layout.name),
                GlueOp::Drop => vs_drop_sym(&layout.name),
            };
            let _ = writeln!(out, "(func {} (param $ptr i32)", sym);
            for v in &layout.variants {
                let live: Vec<&dream_hir::FieldLayout> = v
                    .fields
                    .iter()
                    .filter(|f| {
                        interner.is_rc_tracked(f.ty)
                            || (interner.is_value_type(f.ty) && glue.contains(&f.ty))
                    })
                    .collect();
                if live.is_empty() {
                    continue;
                }
                let _ = writeln!(
                    out,
                    "  (local.get $ptr) (i32.load) (i32.const {}) (i32.eq) (if (then",
                    v.discriminant
                );
                for f in live {
                    emit_field_glue(out, mir, interner, glue, f, op);
                }
                out.push_str("  ))\n");
            }
            out.push_str(")\n");
        }
    }
}

#[derive(Clone, Copy)]
enum GlueOp {
    Retain,
    Drop,
}

/// Emits the retain or release of one field `f` at `$ptr + offset`: a reference field is
/// retained/released by pointer; a value-struct field recurses into its own glue by address; a
/// scalar field needs nothing.
fn emit_field_glue(
    out: &mut String,
    mir: &crate::Mir,
    interner: &TypeInterner,
    glue: &HashSet<TypeId>,
    f: &dream_hir::FieldLayout,
    op: GlueOp,
) {
    let addr = |out: &mut String| {
        out.push_str("  (local.get $ptr)");
        if f.offset > 0 {
            let _ = write!(out, " (i32.const {}) (i32.add)", f.offset);
        }
    };
    if interner.is_rc_tracked(f.ty) {
        addr(out);
        match op {
            GlueOp::Retain => {
                let _ = writeln!(out, " (i32.load) (call {})", retain_call(interner, f.ty));
            }
            GlueOp::Drop => {
                let _ = writeln!(
                    out,
                    " (i32.load) (call {})",
                    release_call(interner, &mir.layouts, f.ty)
                );
            }
        }
    } else if interner.is_value_type(f.ty) && glue.contains(&f.ty) {
        let stripped = f.ty;
        // A nested value field is either a value struct or a value union; resolve its glue name from
        // whichever layout table holds it.
        let name = mir
            .layouts
            .structs
            .get(&stripped)
            .map(|l| l.name.clone())
            .or_else(|| mir.layouts.unions.get(&stripped).map(|u| u.name.clone()));
        if let Some(name) = name {
            addr(out);
            match op {
                GlueOp::Retain => {
                    let _ = writeln!(out, " (call {})", vs_retain_sym(&name));
                }
                GlueOp::Drop => {
                    let _ = writeln!(out, " (call {})", vs_drop_sym(&name));
                }
            }
        }
    }
}

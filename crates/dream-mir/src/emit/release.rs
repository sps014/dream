use super::*;

/// Emits GC mark/update visitors and finalizer dispatch: per-type `$gc_trace_<Type>`,
/// `$gc_trace_array_t*`, `$gc_trace_funcbox`, tag-dispatching `$gc_trace_object`, and
/// `$gc_run_finalizer` (calls `$Type_del` when present). See `docs/compiler/12-tiered-gc.md`.
///
/// `value_trace` — value types with embedded heap refs (inline `$gc_trace_*` calls).
/// `value_glue` — value types with `js`/`del` retain-drop glue (finalizer js unregister).
pub(super) fn emit_gc_funcs(
    out: &mut String,
    mir: &crate::Mir,
    interner: &TypeInterner,
    tags: &HashMap<TypeId, i32>,
    value_trace: &std::collections::HashSet<TypeId>,
    value_glue: &std::collections::HashSet<TypeId>,
) {
    let fn_names: std::collections::HashSet<&str> =
        mir.functions.iter().map(|f| f.name.as_str()).collect();
    let del_of = |name: &str| -> Option<String> {
        let sym = format!("{}_del", name);
        fn_names.contains(sym.as_str()).then_some(sym)
    };

    for (ty, layout) in &mir.layouts.structs {
        let _ = writeln!(out, "(func $gc_trace_{} (param $ptr i32)", layout.name);
        out.push_str("  (local $slot i32) (local $child i32)\n");
        for f in layout
            .fields
            .iter()
            .filter(|f| interner.is_reference(f.ty) && !f.is_weak)
        {
            emit_trace_ref_slot(out, "  ", f.offset);
            emit_trace_ref_child(out, mir, interner, value_trace, f.ty, "  ");
        }
        for f in &layout.fields {
            emit_trace_embedded_value(out, mir, interner, value_trace, f, "  ");
        }
        let _ = ty;
        out.push_str(")\n");
    }

    for layout in mir.layouts.unions.values() {
        let _ = writeln!(out, "(func $gc_trace_{} (param $ptr i32)", layout.name);
        out.push_str("  (local $slot i32) (local $d i32) (local $child i32)\n");
        out.push_str("  (local.get $ptr) (i32.load) (local.set $d)\n");
        for v in &layout.variants {
            let ref_fields: Vec<&dream_hir::FieldLayout> = v
                .fields
                .iter()
                .filter(|f| interner.is_reference(f.ty))
                .collect();
            let value_fields: Vec<&dream_hir::FieldLayout> = v
                .fields
                .iter()
                .filter(|f| interner.is_value_type(f.ty) && value_trace.contains(&f.ty))
                .collect();
            if ref_fields.is_empty() && value_fields.is_empty() {
                continue;
            }
            let _ = writeln!(
                out,
                "  (local.get $d) (i32.const {}) (i32.eq) (if (then",
                v.discriminant
            );
            for f in ref_fields {
                emit_trace_ref_slot(out, "    ", f.offset);
                emit_trace_ref_child(out, mir, interner, value_trace, f.ty, "    ");
            }
            for f in value_fields {
                emit_trace_embedded_value(out, mir, interner, value_trace, f, "    ");
            }
            out.push_str("  ))\n");
        }
        out.push_str(")\n");
    }

    for elem in array_elem_types_for_release(mir, interner) {
        let is_ref = interner.is_reference(elem);
        let is_value = interner.is_value_type(elem);
        if !is_ref && !is_value {
            continue;
        }
        let _ = writeln!(out, "(func $gc_trace_array_t{} (param $ptr i32)", elem.0);
        out.push_str(
            "  (local $slot i32) (local $len i32) (local $i i32) (local $elem i32)\n",
        );
        out.push_str("  (local.get $ptr) (i32.load) (local.set $len)\n");
        out.push_str("  (i32.const 0) (local.set $i)\n");
        out.push_str("  (block $done (loop $scan\n");
        out.push_str("    (local.get $i) (local.get $len) (i32.ge_s) (br_if $done)\n");
        if is_ref {
            out.push_str(
                "    (local.get $ptr) (i32.const 4) (i32.add) (local.get $i) (i32.const 4) (i32.mul) (i32.add) (local.set $slot)\n",
            );
            out.push_str("    (global.get $gc_trace_mode) (i32.const 1) (i32.eq) (if (then\n");
            out.push_str("      (local.get $slot) (i32.load) (call $gc_mark_object)\n");
            out.push_str("    ) (else\n");
            out.push_str("      (local.get $slot) (call $gc_update_slot)\n");
            out.push_str("    ))\n");
        } else if value_trace.contains(&elem) {
            let name = mir.layouts.structs[&elem].name.clone();
            let (stride, _) = dream_hir::scalar_size(interner, elem);
            let _ = writeln!(
                out,
                "    (local.get $ptr) (i32.const 4) (i32.add) (local.get $i) (i32.const {}) (i32.mul) (i32.add) (call $gc_trace_{})",
                stride, name
            );
        }
        out.push_str("    (local.get $i) (i32.const 1) (i32.add) (local.set $i) (br $scan)))\n");
        out.push_str(")\n");
    }

    // Funcbox: word 1 is the env ref (word 0 is the table index).
    out.push_str("(func $gc_trace_funcbox (param $ptr i32)\n");
    out.push_str("  (local $slot i32)\n");
    out.push_str("  (local.get $ptr) (i32.const 4) (i32.add) (local.set $slot)\n");
    out.push_str("  (global.get $gc_trace_mode) (i32.const 1) (i32.eq) (if (then\n");
    out.push_str("    (local.get $slot) (i32.load) (call $gc_mark_object)\n");
    out.push_str("  ) (else\n");
    out.push_str("    (local.get $slot) (call $gc_update_slot)\n");
    out.push_str("  ))\n");
    out.push_str(")\n");

    out.push_str("(func $gc_trace_object (param $ptr i32)\n  (local $tag i32)\n");
    out.push_str("  (local.get $ptr) (i32.eqz) (if (then (return)))\n");
    out.push_str("  (local.get $ptr) (call $object_tag) (local.set $tag)\n");
    write_struct_union_tag_arms(out, mir, tags, |name| {
        format!("(local.get $ptr) (call $gc_trace_{})", name)
    });
    // Arrays: not self-describing about element type — nothing further to do without a typed helper.
    // Funcboxes use malloc tag 0; they are traced only via typed `$gc_trace_funcbox` call sites /
    // object graphs that hold them as strong refs of known static type.
    out.push_str(")\n");

    // Finalizer dispatch: only types that declare `del`.
    out.push_str("(func $gc_run_finalizer (param $ptr i32)\n  (local $tag i32)\n");
    out.push_str("  (local.get $ptr) (i32.eqz) (if (then (return)))\n");
    out.push_str("  (local.get $ptr) (call $object_tag) (local.set $tag)\n");
    for (ty, layout) in &mir.layouts.structs {
        let Some(del) = del_of(&layout.name) else {
            continue;
        };
        let Some(&tag) = tags.get(ty) else {
            continue;
        };
        let _ = writeln!(
            out,
            "  (local.get $tag) (i32.const {}) (i32.eq) (if (then (local.get $ptr) (call ${}) (return)))",
            tag, del
        );
    }
    for (ty, layout) in &mir.layouts.unions {
        let Some(del) = del_of(&layout.name) else {
            continue;
        };
        let Some(&tag) = tags.get(ty) else {
            continue;
        };
        let _ = writeln!(
            out,
            "  (local.get $tag) (i32.const {}) (i32.eq) (if (then (local.get $ptr) (call ${}) (return)))",
            tag, del
        );
    }
    out.push_str(")\n");

    // Unregister host `js` handles stored in unreachable objects (raw i32 ids, not heap refs).
    out.push_str(
        "(func $gc_drop_js_handles (param $ptr i32)\n  (local $tag i32) (local $h i32) (local $d i32)\n",
    );
    out.push_str("  (local.get $ptr) (i32.eqz) (if (then (return)))\n");
    out.push_str("  (local.get $ptr) (call $object_tag) (local.set $tag)\n");
    for (ty, layout) in &mir.layouts.structs {
        let has_js = layout_has_js(mir, interner, value_glue, *ty);
        if !has_js {
            continue;
        }
        let Some(&tag) = tags.get(ty) else {
            continue;
        };
        let _ = writeln!(
            out,
            "  (local.get $tag) (i32.const {}) (i32.eq) (if (then",
            tag
        );
        for f in &layout.fields {
            emit_unregister_js_field(out, mir, interner, value_glue, f, "    ");
        }
        out.push_str("    (return)\n  ))\n");
    }
    for (ty, layout) in &mir.layouts.unions {
        let has_js = layout
            .variants
            .iter()
            .any(|v| v.fields.iter().any(|f| field_has_js(mir, interner, value_glue, f.ty)));
        if !has_js {
            continue;
        }
        let Some(&tag) = tags.get(ty) else {
            continue;
        };
        let _ = writeln!(
            out,
            "  (local.get $tag) (i32.const {}) (i32.eq) (if (then",
            tag
        );
        out.push_str("    (local.get $ptr) (i32.load) (local.set $d)\n");
        for v in &layout.variants {
            let js_fields: Vec<&dream_hir::FieldLayout> = v
                .fields
                .iter()
                .filter(|f| field_has_js(mir, interner, value_glue, f.ty))
                .collect();
            if js_fields.is_empty() {
                continue;
            }
            let _ = writeln!(
                out,
                "    (local.get $d) (i32.const {}) (i32.eq) (if (then",
                v.discriminant
            );
            for f in js_fields {
                emit_unregister_js_field(out, mir, interner, value_glue, f, "      ");
            }
            out.push_str("    ))\n");
        }
        out.push_str("    (return)\n  ))\n");
    }
    out.push_str(")\n");
}

fn field_has_js(
    mir: &crate::Mir,
    interner: &TypeInterner,
    value_glue: &std::collections::HashSet<TypeId>,
    ty: TypeId,
) -> bool {
    if matches!(interner.kind(ty), TyKind::Js) {
        return true;
    }
    if interner.is_value_type(ty) && value_glue.contains(&ty) {
        return layout_has_js(mir, interner, value_glue, ty);
    }
    false
}

fn layout_has_js(
    mir: &crate::Mir,
    interner: &TypeInterner,
    value_glue: &std::collections::HashSet<TypeId>,
    ty: TypeId,
) -> bool {
    if let Some(layout) = mir.layouts.structs.get(&ty) {
        return layout
            .fields
            .iter()
            .any(|f| field_has_js(mir, interner, value_glue, f.ty));
    }
    if let Some(layout) = mir.layouts.unions.get(&ty) {
        return layout.variants.iter().any(|v| {
            v.fields
                .iter()
                .any(|f| field_has_js(mir, interner, value_glue, f.ty))
        });
    }
    false
}

fn emit_unregister_js_field(
    out: &mut String,
    mir: &crate::Mir,
    interner: &TypeInterner,
    value_glue: &std::collections::HashSet<TypeId>,
    f: &dream_hir::FieldLayout,
    indent: &str,
) {
    emit_unregister_js_at(out, mir, interner, value_glue, f.ty, f.offset, indent);
}

fn emit_unregister_js_at(
    out: &mut String,
    mir: &crate::Mir,
    interner: &TypeInterner,
    value_glue: &std::collections::HashSet<TypeId>,
    ty: TypeId,
    base_off: u32,
    indent: &str,
) {
    if matches!(interner.kind(ty), TyKind::Js) {
        let _ = writeln!(out, "{indent}(local.get $ptr)");
        if base_off > 0 {
            let _ = writeln!(out, "{indent}(i32.const {}) (i32.add)", base_off);
        }
        out.push_str(indent);
        out.push_str("(i32.load) (local.tee $h) (if (then (local.get $h) (call $js_unregister)))\n");
        return;
    }
    if !(interner.is_value_type(ty) && value_glue.contains(&ty)) {
        return;
    }
    if let Some(layout) = mir.layouts.structs.get(&ty) {
        for inner in &layout.fields {
            if field_has_js(mir, interner, value_glue, inner.ty) {
                emit_unregister_js_at(
                    out,
                    mir,
                    interner,
                    value_glue,
                    inner.ty,
                    base_off + inner.offset,
                    indent,
                );
            }
        }
    }
}

/// Trace one strong ref slot at `$ptr + offset`: Gen0 update-slot or mark-child depending on
/// `$gc_trace_mode`.
fn emit_trace_ref_slot(out: &mut String, indent: &str, offset: u32) {
    let _ = writeln!(out, "{indent}(local.get $ptr)");
    if offset > 0 {
        let _ = writeln!(out, "{indent}(i32.const {}) (i32.add)", offset);
    }
    let _ = writeln!(out, "{indent}(local.set $slot)");
    let _ = writeln!(out, "{indent}(global.get $gc_trace_mode) (i32.const 1) (i32.eq) (if (then");
    let _ = writeln!(out, "{indent}  (local.get $slot) (i32.load) (call $gc_mark_object)");
    let _ = writeln!(out, "{indent}) (else");
    let _ = writeln!(out, "{indent}  (local.get $slot) (call $gc_update_slot)");
    let _ = writeln!(out, "{indent}))");
}

/// After updating/marking a ref field, recurse into arrays and funcboxes. Those are not covered by
/// `$gc_trace_object` tag dispatch (`TAG_ARRAY` is shared; funcboxes use tag 0), so the parent's
/// static field type must drive a typed visitor — otherwise Gen0 leaves nursery element pointers
/// dangling inside evacuated arrays (Map rehash / JSON parse corruption).
fn emit_trace_ref_child(
    out: &mut String,
    mir: &crate::Mir,
    interner: &TypeInterner,
    value_trace: &std::collections::HashSet<TypeId>,
    ty: TypeId,
    indent: &str,
) {
    match interner.kind(ty) {
        TyKind::Array(elem) => {
            let elem = *elem;
            let needs = interner.is_reference(elem)
                || (interner.is_value_type(elem) && value_trace.contains(&elem));
            if !needs {
                return;
            }
            // Ensure the typed array tracer exists for this elem (emitter already emits for
            // `array_elem_types_for_release`; Map/List field elems are included via layouts).
            let _ = mir;
            let _ = writeln!(
                out,
                "{indent}(local.get $slot) (i32.load) (local.tee $child) (if (then (local.get $child) (call $gc_trace_array_t{})))",
                elem.0
            );
        }
        TyKind::Func(..) => {
            let _ = writeln!(
                out,
                "{indent}(local.get $slot) (i32.load) (local.tee $child) (if (then (local.get $child) (call $gc_trace_funcbox)))"
            );
        }
        _ => {}
    }
}

/// Trace an inline value-struct field by calling its `$gc_trace_<T>`. Value structs with only
/// scalar fields need no visitor; those with ref fields are in `value_trace` and get a
/// `$gc_trace_<Name>` emitted alongside heap classes above.
fn emit_trace_embedded_value(
    out: &mut String,
    mir: &crate::Mir,
    interner: &TypeInterner,
    value_trace: &std::collections::HashSet<TypeId>,
    f: &dream_hir::FieldLayout,
    indent: &str,
) {
    if !interner.is_value_type(f.ty) || !value_trace.contains(&f.ty) {
        return;
    }
    let Some(name) = mir.layouts.structs.get(&f.ty).map(|l| l.name.clone()) else {
        return;
    };
    out.push_str(indent);
    out.push_str("(local.get $ptr)");
    if f.offset > 0 {
        let _ = write!(out, " (i32.const {}) (i32.add)", f.offset);
    }
    let _ = writeln!(out, " (call $gc_trace_{})", name);
}

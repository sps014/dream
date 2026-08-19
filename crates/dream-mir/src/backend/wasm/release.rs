use super::*;

use super::builder::{FuncBuilder, LoadKind, ModuleBuilder, StoreKind, ValType};

/// The `$release_*` symbol that deep-releases a reference value of `ty` (chosen *statically* from the
/// declared type): structs/unions call their generated per-type release, reference-element arrays
/// their element-typed array release, `js` handles call the host `$js_release`, and everything else
/// (strings, scalar arrays, boxed primitives) drops one reference via the generic runtime.
/// `object`-typed values route through the tag-dispatched `$release_object` since their concrete
/// type is unknown until runtime. Callers guard on [`TypeInterner::is_rc_tracked`] (or
/// [`TypeInterner::is_reference`] for heap-only sites) first.
pub(super) fn release_call(interner: &TypeInterner, layouts: &LayoutTable, ty: TypeId) -> String {
    match interner.kind(ty) {
        TyKind::Js => "$js_release".to_string(),
        TyKind::Struct(..) | TyKind::Union(..) => {
            if let Some(l) = layouts.structs.get(&ty) {
                format!("$release_{}", l.name)
            } else if let Some(l) = layouts.unions.get(&ty) {
                format!("$release_{}", l.name)
            } else {
                "$release_object".to_string()
            }
        }
        TyKind::Array(e)
            if interner.is_reference(*e)
                || interner.is_value_type(*e)
                || matches!(interner.kind(*e), TyKind::Js) =>
        {
            format!("$release_array_t{}", e.0)
        }
        // An interface-typed value is a concrete tagged object; release it through the
        // tag-dispatching `$release_object` so the concrete type's deep release runs.
        TyKind::Object | TyKind::Interface(..) => "$release_object".to_string(),
        // Funcboxes deep-release their env word; `$release_generic` would only free the box.
        TyKind::Func(..) => "$release_funcbox".to_string(),
        TyKind::Prim(dream_types::PrimTy::String) => "$string_release".to_string(),
        _ => "$release_generic".to_string(),
    }
}

/// The retain symbol for a reference value of `ty`: `js` handles go through the host `$js_retain`;
/// `@shared class` instances (may be captured into another `WebWorker` thread and retained/released
/// concurrently — see `lock`/point 4 in the shared-memory-WebWorkers plan) go through
/// `$retain_shared` (atomic RMW increment); every other reference type keeps the plain, non-atomic
/// `$retain` fast path.
pub(super) fn destroy_call(interner: &TypeInterner, layouts: &LayoutTable, ty: TypeId) -> String {
    if matches!(interner.kind(ty), TyKind::Js)
        || interner.is_shared_type(ty)
        || matches!(
            interner.kind(ty),
            TyKind::Prim(dream_types::PrimTy::String) | TyKind::Func(..)
        )
    {
        return release_call(interner, layouts, ty);
    }
    match interner.kind(ty) {
        TyKind::Struct(..) | TyKind::Union(..) => {
            if let Some(l) = layouts.structs.get(&ty) {
                format!("$destroy_{}", l.name)
            } else if let Some(l) = layouts.unions.get(&ty) {
                format!("$destroy_{}", l.name)
            } else {
                "$destroy_object".to_string()
            }
        }
        TyKind::Array(e)
            if interner.is_reference(*e)
                || interner.is_value_type(*e)
                || matches!(interner.kind(*e), TyKind::Js) =>
        {
            format!("$destroy_array_t{}", e.0)
        }
        TyKind::Object | TyKind::Interface(..) => "$destroy_object".to_string(),
        _ => release_call(interner, layouts, ty),
    }
}

/// The retain symbol for a reference value of `ty`: `js` handles go through the host `$js_retain`;
/// `@shared class` instances (may be captured into another `WebWorker` thread and retained/released
/// concurrently — see `lock`/point 4 in the shared-memory-WebWorkers plan) go through
/// `$retain_shared` (atomic RMW increment); every other reference type keeps the plain, non-atomic
/// `$retain` fast path.
pub(super) fn retain_call(interner: &TypeInterner, ty: TypeId) -> &'static str {
    if matches!(interner.kind(ty), TyKind::Js) {
        "$js_retain"
    } else if interner.is_shared_type(ty) {
        "$retain_shared"
    } else {
        "$retain"
    }
}

fn rt(name: &str) -> &str {
    name.trim_start_matches('$')
}

fn emit_release_prologue(f: &mut FuncBuilder, unique: bool) {
    f.local_get("ptr");
    f.i32_eqz();
    f.if_();
    f.return_();
    f.end();
    f.local_get("ptr");
    f.i32_const(crate::abi::RC_FROM_DATA as i32);
    f.i32_sub();
    f.local_set("rc");
    if unique {
        f.local_get("rc");
        f.load(LoadKind::I32, 0);
        f.i32_const(i32::MAX);
        f.i32_eq();
        f.if_();
        f.return_();
        f.end();
        return;
    }
    f.local_get("rc");
    f.i32_const(1);
    f.atomic_rmw_sub(0);
    f.i32_const(1);
    f.i32_sub();
    f.local_set("nc");
    f.local_get("nc");
    f.i32_eqz();
    f.if_();
}

fn emit_del_call(f: &mut FuncBuilder, del: Option<&str>) {
    if let Some(d) = del {
        f.local_get("rc");
        f.i32_const(1);
        f.store(StoreKind::I32, 0);
        f.local_get("ptr");
        f.call(d);
    }
}

fn emit_release_ref_field(
    f: &mut FuncBuilder,
    field: &dream_hir::FieldLayout,
    interner: &TypeInterner,
    layouts: &LayoutTable,
) {
    f.local_get("ptr");
    if field.offset > 0 {
        f.i32_const(field.offset as i32);
        f.i32_add();
    }
    f.load(LoadKind::I32, 0);
    f.call(rt(&release_call(interner, layouts, field.ty)));
}

fn emit_embedded_value_drop(
    f: &mut FuncBuilder,
    mir: &crate::Mir,
    interner: &TypeInterner,
    value_glue: &std::collections::HashSet<TypeId>,
    field: &dream_hir::FieldLayout,
) {
    if !interner.is_value_type(field.ty) {
        return;
    }
    if !value_glue.contains(&field.ty) {
        return;
    }
    let Some(name) = mir.layouts.structs.get(&field.ty).map(|l| l.name.clone()) else {
        return;
    };
    f.local_get("ptr");
    if field.offset > 0 {
        f.i32_const(field.offset as i32);
        f.i32_add();
    }
    f.call(rt(&vs_drop_sym(&name)));
}

fn emit_release_weak_field(
    f: &mut FuncBuilder,
    field: &dream_hir::FieldLayout,
    layouts: &LayoutTable,
) {
    let Some(u) = layouts.union(field.ty) else {
        return;
    };
    let Some(some_disc) = u.variant("Some").map(|v| v.discriminant) else {
        return;
    };
    let payload_off = u
        .variant("Some")
        .and_then(|v| v.fields.first())
        .map(|fld| fld.offset)
        .unwrap_or(crate::abi::LEN_PREFIX_SIZE);
    f.local_get("ptr");
    f.i32_const(field.offset as i32);
    f.i32_add();
    f.load(LoadKind::I32, 0);
    f.local_set("__wbox");
    f.local_get("__wbox");
    f.if_();
    f.local_get("__wbox");
    f.load(LoadKind::I32, 0);
    f.i32_const(some_disc);
    f.i32_eq();
    f.if_();
    f.local_get("__wbox");
    f.i32_const(payload_off as i32);
    f.i32_add();
    f.load(LoadKind::I32, 0);
    f.local_get("__wbox");
    f.call("weak_unregister");
    f.end();
    f.local_get("__wbox");
    f.call("free");
    f.end();
}

fn emit_release_unowned_field(f: &mut FuncBuilder, field: &dream_hir::FieldLayout) {
    f.local_get("ptr");
    f.i32_const(field.offset as i32);
    f.i32_add();
    f.load(LoadKind::I32, 0);
    f.local_set("__wbox");
    f.local_get("__wbox");
    f.if_();
    f.local_get("__wbox");
    f.local_get("ptr");
    f.i32_const(field.offset as i32);
    f.i32_add();
    f.call("weak_unregister");
    f.end();
}

pub(super) fn emit_release_funcs(
    m: &mut ModuleBuilder,
    mir: &crate::Mir,
    interner: &TypeInterner,
    tags: &HashMap<TypeId, i32>,
    value_glue: &std::collections::HashSet<TypeId>,
) {
    let fn_names: std::collections::HashSet<&str> =
        mir.functions.iter().map(|f| f.name.as_str()).collect();
    let del_of = |name: &str| -> Option<String> {
        let sym = format!("{}_del", name);
        fn_names.contains(sym.as_str()).then_some(sym)
    };

    for unique in [false, true] {
        let pfx = if unique { "destroy" } else { "release" };
        for layout in mir.layouts.structs.values() {
            let del = del_of(&layout.name);
            let mut f = FuncBuilder::new(format!("{pfx}_{}", layout.name));
            f.param("ptr", ValType::I32);
            f.local("rc", ValType::I32);
            f.local("nc", ValType::I32);
            f.local("__wbox", ValType::I32);
            emit_release_prologue(&mut f, unique);
            emit_del_call(&mut f, del.as_deref());
            for field in layout.fields.iter().filter(|field| {
                interner.is_rc_tracked(field.ty) && !field.is_weak && !field.is_unowned
            }) {
                emit_release_ref_field(&mut f, field, interner, &mir.layouts);
            }
            for field in layout.fields.iter().filter(|field| field.is_weak) {
                emit_release_weak_field(&mut f, field, &mir.layouts);
            }
            for field in layout.fields.iter().filter(|field| field.is_unowned) {
                emit_release_unowned_field(&mut f, field);
            }
            for field in &layout.fields {
                emit_embedded_value_drop(&mut f, mir, interner, value_glue, field);
            }
            f.local_get("ptr");
            f.call("weak_clear_all");
            f.local_get("ptr");
            f.call("free");
            if !unique {
                f.end();
            }
            m.push_func(f);
        }

        for layout in mir.layouts.unions.values() {
            let del = del_of(&layout.name);
            let mut f = FuncBuilder::new(format!("{pfx}_{}", layout.name));
            f.param("ptr", ValType::I32);
            f.local("rc", ValType::I32);
            f.local("nc", ValType::I32);
            f.local("d", ValType::I32);
            emit_release_prologue(&mut f, unique);
            emit_del_call(&mut f, del.as_deref());
            f.local_get("ptr");
            f.load(LoadKind::I32, 0);
            f.local_set("d");
            for v in &layout.variants {
                let ref_fields: Vec<&dream_hir::FieldLayout> = v
                    .fields
                    .iter()
                    .filter(|field| interner.is_rc_tracked(field.ty))
                    .collect();
                if ref_fields.is_empty() {
                    continue;
                }
                f.local_get("d");
                f.i32_const(v.discriminant);
                f.i32_eq();
                f.if_();
                for field in ref_fields {
                    emit_release_ref_field(&mut f, field, interner, &mir.layouts);
                }
                f.end();
            }
            for v in &layout.variants {
                let value_fields: Vec<&dream_hir::FieldLayout> = v
                    .fields
                    .iter()
                    .filter(|field| {
                        interner.is_value_type(field.ty) && value_glue.contains(&field.ty)
                    })
                    .collect();
                if value_fields.is_empty() {
                    continue;
                }
                f.local_get("d");
                f.i32_const(v.discriminant);
                f.i32_eq();
                f.if_();
                for field in value_fields {
                    emit_embedded_value_drop(&mut f, mir, interner, value_glue, field);
                }
                f.end();
            }
            f.local_get("ptr");
            f.call("free");
            if !unique {
                f.end();
            }
            m.push_func(f);
        }

        for elem in array_elem_types(mir, interner) {
            let is_ref = interner.is_rc_tracked(elem);
            let is_value = interner.is_value_type(elem);
            if !is_ref && !is_value {
                continue;
            }
            let mut f = FuncBuilder::new(format!("{pfx}_array_t{}", elem.0));
            f.param("ptr", ValType::I32);
            f.local("rc", ValType::I32);
            f.local("nc", ValType::I32);
            f.local("len", ValType::I32);
            f.local("i", ValType::I32);
            f.local("elem", ValType::I32);
            emit_release_prologue(&mut f, unique);
            if is_ref {
                f.local_get("ptr");
                f.load(LoadKind::I32, 0);
                f.local_set("len");
                f.i32_const(0);
                f.local_set("i");
                f.block("done");
                f.loop_("scan");
                f.local_get("i");
                f.local_get("len");
                f.i32_ge_s();
                f.br_if("done");
                f.local_get("ptr");
                f.i32_const(crate::abi::LEN_PREFIX_SIZE as i32);
                f.i32_add();
                f.local_get("i");
                f.i32_const(crate::abi::LEN_PREFIX_SIZE as i32);
                f.i32_mul();
                f.i32_add();
                f.load(LoadKind::I32, 0);
                f.local_set("elem");
                f.local_get("elem");
                f.if_();
                f.local_get("elem");
                f.call(rt(&release_call(interner, &mir.layouts, elem)));
                f.end();
                f.local_get("i");
                f.i32_const(1);
                f.i32_add();
                f.local_set("i");
                f.br("scan");
                f.end();
                f.end();
            } else if value_glue.contains(&elem) {
                let name = mir.layouts.structs[&elem].name.clone();
                let (stride, _) = dream_hir::scalar_size(interner, elem);
                f.local_get("ptr");
                f.load(LoadKind::I32, 0);
                f.local_set("len");
                f.i32_const(0);
                f.local_set("i");
                f.block("done");
                f.loop_("scan");
                f.local_get("i");
                f.local_get("len");
                f.i32_ge_s();
                f.br_if("done");
                f.local_get("ptr");
                f.i32_const(crate::abi::LEN_PREFIX_SIZE as i32);
                f.i32_add();
                f.local_get("i");
                f.i32_const(stride as i32);
                f.i32_mul();
                f.i32_add();
                f.call(rt(&vs_drop_sym(&name)));
                f.local_get("i");
                f.i32_const(1);
                f.i32_add();
                f.local_set("i");
                f.br("scan");
                f.end();
                f.end();
            }
            f.local_get("ptr");
            f.call("free");
            if !unique {
                f.end();
            }
            m.push_func(f);
        }

        let mut f = FuncBuilder::new(format!("{pfx}_object"));
        f.param("ptr", ValType::I32);
        f.local("tag", ValType::I32);
        f.local_get("ptr");
        f.i32_eqz();
        f.if_();
        f.return_();
        f.end();
        f.local_get("ptr");
        f.call("object_tag");
        f.local_set("tag");
        for (ty, layout) in &mir.layouts.structs {
            if let Some(&tag) = tags.get(ty) {
                f.local_get("tag");
                f.i32_const(tag);
                f.i32_eq();
                f.if_();
                f.local_get("ptr");
                f.call(&format!("{pfx}_{}", layout.name));
                f.return_();
                f.end();
            }
        }
        for (ty, layout) in &mir.layouts.unions {
            if let Some(&tag) = tags.get(ty) {
                f.local_get("tag");
                f.i32_const(tag);
                f.i32_eq();
                f.if_();
                f.local_get("ptr");
                f.call(&format!("{pfx}_{}", layout.name));
                f.return_();
                f.end();
            }
        }
        f.local_get("tag");
        f.i32_const(crate::abi::TAG_STRING);
        f.i32_eq();
        f.if_();
        f.local_get("ptr");
        f.call("string_release");
        f.return_();
        f.end();
        f.local_get("ptr");
        f.call("release_generic");
        m.push_func(f);
    }
}

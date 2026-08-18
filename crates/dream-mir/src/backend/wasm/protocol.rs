use super::builder::{FuncBuilder, LoadKind, ModuleBuilder, ValType};
use super::*;

/// Emits the object-protocol runtime that depends on the user's types: one default `$<Type>_to_string`
/// per struct, plus the tag-dispatching `$object_to_string` and `$print_object` routers. Struct
/// `to_string` renders as `Type { field: value, ... }`, recursing into reference fields via
/// `$object_to_string`.
pub(super) fn emit_object_protocol(
    m: &mut ModuleBuilder,
    mir: &crate::Mir,
    interner: &TypeInterner,
    strings: &IndexMap<String, u32>,
    tags: &HashMap<TypeId, i32>,
) {
    let user_syms: std::collections::HashSet<String> =
        mir.functions.iter().map(func_symbol).collect();
    let has_override =
        |name: &str, method: &str| user_syms.contains(&format!("{}_{}", name, method));
    for (ty, layout) in &mir.layouts.structs {
        if !has_override(&layout.name, "to_string") {
            if matches!(interner.kind(*ty), TyKind::Tuple(_)) {
                emit_tuple_to_string(m, layout, &mir.layouts, interner, strings);
            } else {
                emit_struct_to_string(m, layout, &mir.layouts, interner, strings);
            }
        }
    }
    for layout in mir.layouts.unions.values() {
        if !has_override(&layout.name, "to_string") {
            emit_union_to_string(m, layout, &mir.layouts, interner, strings);
        }
    }
    for elem in array_elem_types(mir, interner) {
        emit_array_to_string(m, elem, interner, strings);
    }
    emit_object_to_string(m, mir, strings, tags);
    let mut print = FuncBuilder::new("print_object");
    print.param("ptr", ValType::I32);
    print.local_get("ptr");
    print.call("object_to_string");
    print.call("print_string");
    m.push_func(print);
    for layout in mir.layouts.structs.values() {
        if !has_override(&layout.name, "hash_code") {
            emit_struct_hash_code(m, layout, interner);
        }
    }
    for layout in mir.layouts.unions.values() {
        if !has_override(&layout.name, "hash_code") {
            emit_union_hash_code(m, layout, interner);
        }
    }
    emit_object_hash_code(m, mir, tags);
}

fn i32_fn(name: &str, param: &str) -> FuncBuilder {
    let mut f = FuncBuilder::new(name);
    f.param(param, ValType::I32);
    f.result(ValType::I32);
    f
}

fn rt(name: &str) -> &str {
    name.trim_start_matches('$')
}

fn ptr_off(f: &mut FuncBuilder, base: &str, offset: u32) {
    f.local_get(base);
    if offset > 0 {
        f.i32_const(offset as i32);
        f.i32_add();
    }
}

fn concat_into_res(f: &mut FuncBuilder, addr: u32) {
    f.local_get("res");
    f.i32_const(addr as i32);
    f.call("concat_strings");
    f.local_set("res");
}

fn emit_prim_hash(f: &mut FuncBuilder, hash: &str) {
    match hash {
        "" => {}
        "(i32.reinterpret_f32)" => f.i32_reinterpret_f32(),
        "(call $hash_double)" => f.call("hash_double"),
        "(call $hash_long)" => f.call("hash_long"),
        "(call $hash_string)" => f.call("hash_string"),
        other => crate::internal_error!("unknown prim hash {other}"),
    }
}

fn emit_hash_of(f: &mut FuncBuilder, interner: &TypeInterner, ty: TypeId) {
    match interner.kind(ty) {
        TyKind::Prim(p) => emit_prim_hash(f, prim_info(*p).hash),
        TyKind::Enum(_) => {}
        _ => f.call("object_hash_code"),
    }
}

fn write_tag_arm(f: &mut FuncBuilder, tag: i32, body: impl FnOnce(&mut FuncBuilder)) {
    f.local_get("tag");
    f.i32_const(tag);
    f.i32_eq();
    f.if_();
    body(f);
    f.return_();
    f.end();
}

fn write_struct_union_tag_arms(
    f: &mut FuncBuilder,
    mir: &crate::Mir,
    tags: &HashMap<TypeId, i32>,
    mut body: impl FnMut(&mut FuncBuilder, &str),
) {
    for (ty, layout) in &mir.layouts.structs {
        if let Some(&tag) = tags.get(ty) {
            write_tag_arm(f, tag, |fb| body(fb, &layout.name));
        }
    }
    for (ty, layout) in &mir.layouts.unions {
        if let Some(&tag) = tags.get(ty) {
            write_tag_arm(f, tag, |fb| body(fb, &layout.name));
        }
    }
}

fn emit_to_string_value(
    f: &mut FuncBuilder,
    base: &str,
    field: &dream_hir::FieldLayout,
    layouts: &LayoutTable,
    interner: &TypeInterner,
) {
    ptr_off(f, base, field.offset);
    if interner.is_value_type(field.ty) {
        let name = layouts
            .get(field.ty)
            .map(|l| l.name.as_str())
            .or_else(|| layouts.union(field.ty).map(|u| u.name.as_str()));
        if let Some(name) = name {
            f.call(&format!("{name}_to_string"));
        } else {
            f.load(load_kind_for(interner, field.ty), 0);
            f.call("object_to_string");
        }
    } else {
        f.load(load_kind_for(interner, field.ty), 0);
        if let Some(call) = value_to_string_call(interner, field.ty) {
            f.call(rt(&call));
        }
    }
}

fn emit_to_string_field(
    f: &mut FuncBuilder,
    label: u32,
    field: &dream_hir::FieldLayout,
    layouts: &LayoutTable,
    interner: &TypeInterner,
) {
    concat_into_res(f, label);
    f.local_get("res");
    emit_to_string_value(f, "this", field, layouts, interner);
    f.call("concat_strings");
    f.local_set("res");
}

fn emit_to_string_elem(
    f: &mut FuncBuilder,
    field: &dream_hir::FieldLayout,
    layouts: &LayoutTable,
    interner: &TypeInterner,
) {
    f.local_get("res");
    emit_to_string_value(f, "this", field, layouts, interner);
    f.call("concat_strings");
    f.local_set("res");
}

fn emit_hash_fields(
    f: &mut FuncBuilder,
    fields: &[dream_hir::FieldLayout],
    interner: &TypeInterner,
) {
    for field in fields {
        f.local_get("h");
        f.i32_const(31);
        f.i32_mul();
        ptr_off(f, "this", field.offset);
        f.load(load_kind_for(interner, field.ty), 0);
        emit_hash_of(f, interner, field.ty);
        f.i32_add();
        f.local_set("h");
    }
}

fn emit_struct_hash_code(
    m: &mut ModuleBuilder,
    layout: &dream_hir::TypeLayout,
    interner: &TypeInterner,
) {
    let mut f = i32_fn(&format!("{}_hash_code", layout.name), "this");
    f.local("h", ValType::I32);
    f.i32_const(17);
    f.local_set("h");
    emit_hash_fields(&mut f, &layout.fields, interner);
    f.local_get("h");
    m.push_func(f);
}

fn emit_union_hash_code(
    m: &mut ModuleBuilder,
    layout: &dream_hir::UnionLayout,
    interner: &TypeInterner,
) {
    let mut f = i32_fn(&format!("{}_hash_code", layout.name), "this");
    f.local("h", ValType::I32);
    f.local("d", ValType::I32);
    f.local_get("this");
    f.load(LoadKind::I32, 0);
    f.local_set("d");
    f.i32_const(17);
    f.i32_const(31);
    f.i32_mul();
    f.local_get("d");
    f.i32_add();
    f.local_set("h");
    for variant in &layout.variants {
        f.local_get("d");
        f.i32_const(variant.discriminant);
        f.i32_eq();
        f.if_();
        emit_hash_fields(&mut f, &variant.fields, interner);
        f.end();
    }
    f.local_get("h");
    m.push_func(f);
}

fn emit_object_hash_code(m: &mut ModuleBuilder, mir: &crate::Mir, tags: &HashMap<TypeId, i32>) {
    let mut f = i32_fn("object_hash_code", "ptr");
    f.local("tag", ValType::I32);
    f.local_get("ptr");
    f.i32_eqz();
    f.if_();
    f.i32_const(0);
    f.return_();
    f.end();
    f.local_get("ptr");
    f.call("object_tag");
    f.local_set("tag");
    for e in PRIM_TABLE {
        write_tag_arm(&mut f, e.tag, |fb| {
            fb.local_get("ptr");
            if let Some(unbox) = e.unbox_fn {
                fb.call(rt(unbox));
            }
            emit_prim_hash(fb, e.hash);
        });
    }
    write_struct_union_tag_arms(&mut f, mir, tags, |fb, name| {
        fb.local_get("ptr");
        fb.call(&format!("{name}_hash_code"));
    });
    f.local_get("ptr");
    m.push_func(f);
}

fn emit_struct_to_string(
    m: &mut ModuleBuilder,
    layout: &dream_hir::TypeLayout,
    layouts: &LayoutTable,
    interner: &TypeInterner,
    strings: &IndexMap<String, u32>,
) {
    let prefix = format!("{} {{ ", layout.name);
    let mut f = i32_fn(&format!("{}_to_string", layout.name), "this");
    f.local("res", ValType::I32);
    f.i32_const(strings[&prefix] as i32);
    f.local_set("res");
    for (i, field) in layout.fields.iter().enumerate() {
        let label = if i == 0 {
            format!("{}: ", field.name)
        } else {
            format!(", {}: ", field.name)
        };
        emit_to_string_field(&mut f, strings[&label], field, layouts, interner);
    }
    f.local_get("res");
    f.i32_const(strings[" }"] as i32);
    f.call("concat_strings");
    m.push_func(f);
}

fn emit_tuple_to_string(
    m: &mut ModuleBuilder,
    layout: &dream_hir::TypeLayout,
    layouts: &LayoutTable,
    interner: &TypeInterner,
    strings: &IndexMap<String, u32>,
) {
    let mut f = i32_fn(&format!("{}_to_string", layout.name), "this");
    f.local("res", ValType::I32);
    f.i32_const(strings["("] as i32);
    f.local_set("res");
    for (i, field) in layout.fields.iter().enumerate() {
        if i > 0 {
            concat_into_res(&mut f, strings[", "]);
        }
        emit_to_string_elem(&mut f, field, layouts, interner);
    }
    f.local_get("res");
    f.i32_const(strings[")"] as i32);
    f.call("concat_strings");
    m.push_func(f);
}

fn emit_union_to_string(
    m: &mut ModuleBuilder,
    layout: &dream_hir::UnionLayout,
    layouts: &LayoutTable,
    interner: &TypeInterner,
    strings: &IndexMap<String, u32>,
) {
    let mut f = i32_fn(&format!("{}_to_string", layout.name), "this");
    f.local("res", ValType::I32);
    f.local("d", ValType::I32);
    f.i32_const(strings["<object>"] as i32);
    f.local_set("res");
    f.local_get("this");
    f.load(LoadKind::I32, 0);
    f.local_set("d");
    for variant in &layout.variants {
        let (prefix, labels, suffix) = union_variant_pieces(variant);
        f.local_get("d");
        f.i32_const(variant.discriminant);
        f.i32_eq();
        f.if_();
        f.i32_const(strings[&prefix] as i32);
        f.local_set("res");
        for (idx, field) in variant.fields.iter().enumerate() {
            emit_to_string_field(&mut f, strings[&labels[idx]], field, layouts, interner);
        }
        concat_into_res(&mut f, strings[&suffix]);
        f.end();
    }
    f.local_get("res");
    m.push_func(f);
}

/// The distinct array **element** types that need a generated `$array_to_string_t<id>`.
pub(super) fn array_elem_types(mir: &crate::Mir, interner: &TypeInterner) -> Vec<TypeId> {
    let mut order: Vec<TypeId> = Vec::new();
    for layout in mir.layouts.structs.values() {
        for field in &layout.fields {
            push_array_elem(&mut order, interner, field.ty);
        }
    }
    for layout in mir.layouts.unions.values() {
        for v in &layout.variants {
            for field in &v.fields {
                push_array_elem(&mut order, interner, field.ty);
            }
        }
    }
    for fun in &mir.functions {
        for l in &fun.locals {
            push_array_elem(&mut order, interner, l.ty);
        }
        for b in &fun.blocks {
            for s in &b.stmts {
                if let Statement::Print { ty, .. } = s {
                    push_array_elem(&mut order, interner, *ty);
                }
            }
        }
    }
    for g in &mir.globals {
        push_array_elem(&mut order, interner, g.ty);
    }
    for (_id, kind) in interner.iter_kinds() {
        if let TyKind::Array(elem) = kind {
            if !order.contains(elem) {
                order.push(*elem);
            }
        }
    }
    let mut i = 0;
    while i < order.len() {
        let cur = order[i];
        push_array_elem(&mut order, interner, cur);
        i += 1;
    }
    order
}

pub(super) fn push_array_elem(order: &mut Vec<TypeId>, interner: &TypeInterner, ty: TypeId) {
    if let Some(e) = interner.unwrap_array(ty) {
        if !order.contains(&e) {
            order.push(e);
        }
    }
}

fn emit_array_to_string(
    m: &mut ModuleBuilder,
    elem: TypeId,
    interner: &TypeInterner,
    strings: &IndexMap<String, u32>,
) {
    let (esize, _) = scalar_size(interner, elem);
    let mut f = i32_fn(rt(&array_to_string_sym(elem)), "ptr");
    f.local("res", ValType::I32);
    f.local("len", ValType::I32);
    f.local("i", ValType::I32);
    f.i32_const(strings["["] as i32);
    f.local_set("res");
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
    f.local_get("i");
    f.i32_const(0);
    f.i32_gt_s();
    f.if_();
    concat_into_res(&mut f, strings[", "]);
    f.end();
    f.local_get("res");
    f.local_get("ptr");
    f.i32_const(crate::abi::LEN_PREFIX_SIZE as i32);
    f.i32_add();
    if esize == 1 {
        f.local_get("i");
        f.i32_add();
    } else {
        f.local_get("i");
        f.i32_const(esize as i32);
        f.i32_mul();
        f.i32_add();
    }
    f.load(load_kind_for(interner, elem), 0);
    if let Some(call) = value_to_string_call(interner, elem) {
        f.call(rt(&call));
    }
    f.call("concat_strings");
    f.local_set("res");
    f.local_get("i");
    f.i32_const(1);
    f.i32_add();
    f.local_set("i");
    f.br("scan");
    f.end();
    f.end();
    f.local_get("res");
    f.i32_const(strings["]"] as i32);
    f.call("concat_strings");
    m.push_func(f);
}

fn emit_object_to_string(
    m: &mut ModuleBuilder,
    mir: &crate::Mir,
    strings: &IndexMap<String, u32>,
    tags: &HashMap<TypeId, i32>,
) {
    let mut f = i32_fn("object_to_string", "ptr");
    f.local("tag", ValType::I32);
    f.local_get("ptr");
    f.i32_eqz();
    f.if_();
    f.i32_const(strings["null"] as i32);
    f.return_();
    f.end();
    f.local_get("ptr");
    f.call("object_tag");
    f.local_set("tag");
    for e in PRIM_TABLE {
        write_tag_arm(&mut f, e.tag, |fb| {
            fb.local_get("ptr");
            if let (Some(unbox), Some(to_str)) = (e.unbox_fn, e.to_string) {
                fb.call(rt(unbox));
                fb.call(rt(to_str));
            }
        });
    }
    write_struct_union_tag_arms(&mut f, mir, tags, |fb, name| {
        fb.local_get("ptr");
        fb.call(&format!("{name}_to_string"));
    });
    f.i32_const(strings["<object>"] as i32);
    m.push_func(f);
}

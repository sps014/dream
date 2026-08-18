use crate::backend::c::ctx::Cx;
use crate::backend::c::rvalue::{hash_code_expr, to_string_fn};
use crate::backend::c::types::c_ident;
use crate::backend::wasm::func_symbol;
use dream_types::{TyKind, TypeId};

pub(super) fn emit_protocol(out: &mut String, cx: &Cx<'_>) {
    let mut array_elems: Vec<_> = cx
        .mir
        .functions
        .iter()
        .flat_map(|f| f.locals.iter().map(|decl| decl.ty))
        .filter_map(|ty| match cx.interner.kind(ty) {
            TyKind::Array(elem) => Some(*elem),
            _ => None,
        })
        .collect();
    array_elems.extend(
        cx.native
            .structs
            .values()
            .flat_map(|layout| layout.fields.iter().map(|field| field.ty))
            .filter_map(|ty| match cx.interner.kind(ty) {
                TyKind::Array(elem) => Some(*elem),
                _ => None,
            }),
    );
    array_elems.sort_by_key(|ty| ty.0);
    array_elems.dedup();
    let user: std::collections::HashSet<String> =
        cx.mir.functions.iter().map(func_symbol).collect();
    for elem in &array_elems {
        out.push_str(&format!(
            "dream_ptr {}(dream_ptr p);\n",
            c_ident(&format!("array_to_string_t{}", elem.0))
        ));
    }
    for layout in cx.native.structs.values() {
        let sym = format!("{}_to_string", layout.name);
        if !user.contains(&sym) {
            out.push_str(&format!("dream_ptr {}(dream_ptr p);\n", c_ident(&sym)));
        }
        let hash = format!("{}_hash_code", layout.name);
        if !user.contains(&hash) {
            out.push_str(&format!("int32_t {}(dream_ptr p);\n", c_ident(&hash)));
        }
    }
    for layout in cx.native.unions.values() {
        let sym = format!("{}_to_string", layout.name);
        if !user.contains(&sym) {
            out.push_str(&format!("dream_ptr {}(dream_ptr p);\n", c_ident(&sym)));
        }
        let hash = format!("{}_hash_code", layout.name);
        if !user.contains(&hash) {
            out.push_str(&format!("int32_t {}(dream_ptr p);\n", c_ident(&hash)));
        }
    }
    if !array_elems.is_empty() || !cx.native.structs.is_empty() || !cx.native.unions.is_empty() {
        out.push('\n');
    }
    for elem in array_elems {
        emit_array_to_string(out, cx, elem);
    }
    if !out.is_empty() {
        out.push('\n');
    }
    for (ty, layout) in &cx.native.structs {
        let sym = format!("{}_to_string", layout.name);
        if user.contains(&sym) {
            continue;
        }
        emit_struct_to_string(out, cx, *ty, &layout.name, &layout.fields);
    }
    for (ty, layout) in &cx.native.unions {
        let sym = format!("{}_to_string", layout.name);
        if user.contains(&sym) {
            continue;
        }
        emit_union_to_string(out, cx, *ty, &layout.name);
    }
    for (ty, layout) in &cx.native.structs {
        let hash = format!("{}_hash_code", layout.name);
        if !user.contains(&hash) {
            emit_struct_hash_code(out, cx, *ty, layout);
        }
    }
    for (ty, layout) in &cx.native.unions {
        let hash = format!("{}_hash_code", layout.name);
        if !user.contains(&hash) {
            emit_union_hash_code(out, cx, *ty, layout);
        }
    }
    emit_object_to_string_router(out, cx);
    emit_object_hash_code_router(out, cx);
}

fn emit_array_to_string(out: &mut String, cx: &Cx<'_>, elem: TypeId) {
    let fn_name = c_ident(&format!("array_to_string_t{}", elem.0));
    let es = crate::backend::c::types::elem_size(cx, elem);
    let cast = crate::backend::c::types::load_cast(cx, elem);
    let conv = to_string_fn(cx, elem);
    out.push_str(&format!(
        "dream_ptr {fn_name}(dream_ptr p) {{\n  int32_t n = p ? *(int32_t *)dream_p(p) : 0;\n  int32_t i;\n  dream_ptr r = {};\n",
        cx.str_sym("[")
    ));
    out.push_str("  for (i = 0; i < n; i++) {\n");
    out.push_str(&format!(
        "    if (i) {{ dream_ptr __c = dream_concat_strings(r, {}); dream_release(r); r = __c; }}\n",
        cx.str_sym(", ")
    ));
    if cx.interner.is_value_type(elem) {
        out.push_str(&format!(
            "    {{ dream_ptr __p = {conv}((dream_ptr)((char *)dream_p(p) + 4 + (size_t)i * {es})); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }}\n"
        ));
    } else if conv.is_empty() {
        out.push_str(&format!(
            "    {{ dream_ptr __c = dream_concat_strings(r, *({cast} *)((char *)dream_p(p) + 4 + (size_t)i * {es})); dream_release(r); r = __c; }}\n"
        ));
    } else {
        out.push_str(&format!(
            "    {{ dream_ptr __p = {conv}(*({cast} *)((char *)dream_p(p) + 4 + (size_t)i * {es})); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }}\n"
        ));
    }
    out.push_str("  }\n");
    out.push_str(&format!(
        "  {{ dream_ptr __c = dream_concat_strings(r, {}); dream_release(r); return __c; }}\n}}\n\n",
        cx.str_sym("]")
    ));
}

fn emit_struct_to_string(
    out: &mut String,
    cx: &Cx<'_>,
    _ty: TypeId,
    name: &str,
    fields: &[dream_hir::FieldLayout],
) {
    let fn_name = c_ident(&format!("{name}_to_string"));
    out.push_str(&format!("dream_ptr {fn_name}(dream_ptr p) {{\n"));
    out.push_str("  if (!p) return ");
    out.push_str(cx.str_sym("null"));
    out.push_str(";\n  dream_ptr r = ");
    let start = if matches!(cx.interner.kind(_ty), TyKind::Tuple(_)) {
        "(".into()
    } else {
        format!("{name} {{ ")
    };
    out.push_str(cx.str_sym(&start));
    out.push_str(";\n");
    for (i, f) in fields.iter().enumerate() {
        let label = if matches!(cx.interner.kind(_ty), TyKind::Tuple(_)) {
            if i == 0 {
                String::new()
            } else {
                ", ".into()
            }
        } else if i == 0 {
            format!("{}: ", f.name)
        } else {
            format!(", {}: ", f.name)
        };
        out.push_str(&format!(
            "  {{ dream_ptr __c = dream_concat_strings(r, {}); dream_release(r); r = __c; }}\n",
            cx.str_sym(&label)
        ));
        let value = if cx.interner.is_value_type(f.ty) {
            format!("(dream_ptr)((char *)dream_p(p) + {})", f.offset)
        } else {
            let cast = crate::backend::c::types::load_cast(cx, f.ty);
            format!("*({cast} *)((char *)dream_p(p) + {})", f.offset)
        };
        let text = to_string_fn(cx, f.ty);
        if text.is_empty() {
            out.push_str(&format!(
                "  {{ dream_ptr __c = dream_concat_strings(r, {value}); dream_release(r); r = __c; }}\n"
            ));
        } else {
            out.push_str(&format!(
                "  {{ dream_ptr __p = {text}({value}); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }}\n"
            ));
        }
    }
    let end = if matches!(cx.interner.kind(_ty), TyKind::Tuple(_)) {
        ")"
    } else {
        " }"
    };
    out.push_str(&format!(
        "  {{ dream_ptr __c = dream_concat_strings(r, {}); dream_release(r); return __c; }}\n}}\n\n",
        cx.str_sym(end)
    ));
}

fn emit_union_to_string(out: &mut String, cx: &Cx<'_>, ty: TypeId, name: &str) {
    let fn_name = c_ident(&format!("{name}_to_string"));
    let Some(layout) = cx.native.unions.get(&ty) else {
        out.push_str(&format!(
            "dream_ptr {fn_name}(dream_ptr p) {{\n  (void)p;\n  return {};\n}}\n\n",
            cx.str_sym("<object>")
        ));
        return;
    };
    out.push_str(&format!("dream_ptr {fn_name}(dream_ptr p) {{\n"));
    out.push_str("  int32_t d;\n  dream_ptr r;\n  if (!p) return ");
    out.push_str(cx.str_sym("null"));
    out.push_str(";\n  d = *(int32_t *)dream_p(p);\n  r = ");
    out.push_str(cx.str_sym("<object>"));
    out.push_str(";\n  switch (d) {\n");
    for variant in &layout.variants {
        let (prefix, labels, suffix) = union_variant_pieces(variant);
        out.push_str(&format!("    case {}: {{\n", variant.discriminant));
        out.push_str(&format!("      r = {};\n", cx.str_sym(&prefix)));
        for (i, f) in variant.fields.iter().enumerate() {
            out.push_str(&format!(
                "      {{ dream_ptr __c = dream_concat_strings(r, {}); dream_release(r); r = __c; }}\n",
                cx.str_sym(&labels[i])
            ));
            let value = field_value_expr(cx, f);
            let text = to_string_fn(cx, f.ty);
            if text.is_empty() {
                out.push_str(&format!(
                    "      {{ dream_ptr __c = dream_concat_strings(r, {value}); dream_release(r); r = __c; }}\n"
                ));
            } else {
                out.push_str(&format!(
                    "      {{ dream_ptr __p = {text}({value}); dream_ptr __c = dream_concat_strings(r, __p); dream_release(r); dream_release(__p); r = __c; }}\n"
                ));
            }
        }
        if !suffix.is_empty() {
            out.push_str(&format!(
                "      {{ dream_ptr __c = dream_concat_strings(r, {}); dream_release(r); r = __c; }}\n",
                cx.str_sym(&suffix)
            ));
        }
        out.push_str("      break;\n    }\n");
    }
    out.push_str("    default: break;\n  }\n  return r;\n}\n\n");
}

fn union_variant_pieces(v: &dream_hir::UnionVariant) -> (String, Vec<String>, String) {
    if v.fields.is_empty() {
        return (v.name.clone(), Vec::new(), String::new());
    }
    let prefix = format!("{}(", v.name);
    let labels = v
        .fields
        .iter()
        .enumerate()
        .map(|(i, f)| {
            if i == 0 {
                format!("{}: ", f.name)
            } else {
                format!(", {}: ", f.name)
            }
        })
        .collect();
    (prefix, labels, ")".to_string())
}

fn field_value_expr(cx: &Cx<'_>, f: &dream_hir::FieldLayout) -> String {
    if cx.interner.is_value_type(f.ty) {
        format!("(dream_ptr)((char *)dream_p(p) + {})", f.offset)
    } else {
        let cast = crate::backend::c::types::load_cast(cx, f.ty);
        format!("*({cast} *)((char *)dream_p(p) + {})", f.offset)
    }
}

fn emit_struct_hash_code(
    out: &mut String,
    cx: &Cx<'_>,
    _ty: TypeId,
    layout: &dream_hir::TypeLayout,
) {
    let fn_name = c_ident(&format!("{}_hash_code", layout.name));
    out.push_str(&format!("int32_t {fn_name}(dream_ptr p) {{\n  int32_t h = 17;\n"));
    for f in &layout.fields {
        emit_hash_field(out, cx, f);
    }
    out.push_str("  return h;\n}\n\n");
}

fn emit_union_hash_code(
    out: &mut String,
    cx: &Cx<'_>,
    _ty: TypeId,
    layout: &dream_hir::UnionLayout,
) {
    let fn_name = c_ident(&format!("{}_hash_code", layout.name));
    out.push_str(&format!(
        "int32_t {fn_name}(dream_ptr p) {{\n  int32_t d = p ? *(int32_t *)dream_p(p) : 0;\n  int32_t h = 17 * 31 + d;\n  switch (d) {{\n"
    ));
    for variant in &layout.variants {
        out.push_str(&format!("    case {}: {{\n", variant.discriminant));
        for f in &variant.fields {
            emit_hash_field(out, cx, f);
        }
        out.push_str("      break;\n    }\n");
    }
    out.push_str("    default: break;\n  }\n  return h;\n}\n\n");
}

fn emit_hash_field(out: &mut String, cx: &Cx<'_>, f: &dream_hir::FieldLayout) {
    let value = field_value_expr(cx, f);
    let hashed = hash_code_expr(cx, f.ty, &value);
    out.push_str(&format!("      h = h * 31 + {hashed};\n"));
}

fn emit_object_hash_code_router(out: &mut String, cx: &Cx<'_>) {
    out.push_str("int32_t dream_object_hash_code(dream_ptr p) {\n");
    out.push_str("  int32_t tag;\n  if (!p) return 0;\n  tag = dream_object_tag(p);\n  switch (tag) {\n");
    out.push_str("    case TAG_INT: case TAG_UINT: case TAG_BOOL: case TAG_CHAR: case TAG_BYTE: return *(int32_t *)dream_p(p);\n");
    out.push_str("    case TAG_LONG: case TAG_ULONG: return dream_hash_long(*(int64_t *)dream_p(p));\n");
    out.push_str("    case TAG_FLOAT: return dream_bitcast_f32(*(float *)dream_p(p));\n");
    out.push_str("    case TAG_DOUBLE: return dream_hash_double(*(double *)dream_p(p));\n");
    out.push_str("    case TAG_STRING: return dream_string_hash(p);\n");
    let mut tagged: Vec<_> = cx.tags.iter().collect();
    tagged.sort_by_key(|(_, t)| **t);
    for (ty, tag) in tagged {
        if let Some(l) = cx.native.structs.get(ty) {
            if matches!(cx.interner.kind(*ty), TyKind::Tuple(_)) {
                continue;
            }
            let fn_name = c_ident(&format!("{}_hash_code", l.name));
            out.push_str(&format!("    case {tag}: return {fn_name}(p);\n"));
        } else if let Some(l) = cx.native.unions.get(ty) {
            let fn_name = c_ident(&format!("{}_hash_code", l.name));
            out.push_str(&format!("    case {tag}: return {fn_name}(p);\n"));
        }
    }
    out.push_str("    default: return (int32_t)(uintptr_t)p;\n  }\n}\n\n");
}

fn emit_object_to_string_router(out: &mut String, cx: &Cx<'_>) {
    out.push_str("dream_ptr dream_object_to_string(dream_ptr p) {\n");
    out.push_str("  int32_t tag;\n  if (!p) return ");
    out.push_str(cx.str_sym("null"));
    out.push_str(";\n  tag = dream_object_tag(p);\n  switch (tag) {\n");
    out.push_str("    case TAG_INT: return dream_int_to_string(*(int32_t *)dream_p(p));\n");
    out.push_str("    case TAG_UINT: return dream_uint_to_string(*(int32_t *)dream_p(p));\n");
    out.push_str("    case TAG_LONG: return dream_long_to_string(*(int64_t *)dream_p(p));\n");
    out.push_str("    case TAG_ULONG: return dream_ulong_to_string(*(int64_t *)dream_p(p));\n");
    out.push_str("    case TAG_BYTE: return dream_byte_to_string(*(int32_t *)dream_p(p));\n");
    out.push_str("    case TAG_BOOL: return dream_bool_to_string(*(int32_t *)dream_p(p));\n");
    out.push_str("    case TAG_CHAR: return dream_char_to_string(*(int32_t *)dream_p(p));\n");
    out.push_str("    case TAG_FLOAT: return dream_float_to_string(*(float *)dream_p(p));\n");
    out.push_str("    case TAG_DOUBLE: return dream_double_to_string(*(double *)dream_p(p));\n");
    out.push_str("    case TAG_STRING: dream_retain(p); return p;\n");
    out.push_str("    case TAG_ARRAY: return dream_array_to_string(p);\n");
    let mut tagged: Vec<_> = cx.tags.iter().collect();
    tagged.sort_by_key(|(_, t)| **t);
    for (ty, tag) in tagged {
        if let Some(l) = cx.native.structs.get(ty) {
            if matches!(cx.interner.kind(*ty), TyKind::Tuple(_)) {
                continue;
            }
            let fn_name = c_ident(&format!("{}_to_string", l.name));
            out.push_str(&format!("    case {tag}: return {fn_name}(p);\n"));
        } else if let Some(l) = cx.native.unions.get(ty) {
            let fn_name = c_ident(&format!("{}_to_string", l.name));
            out.push_str(&format!("    case {tag}: return {fn_name}(p);\n"));
        }
    }
    out.push_str("    default: return ");
    out.push_str(cx.str_sym("<object>"));
    out.push_str(";\n  }\n}\n\n");
    out.push_str(
        "void dream_print_object(dream_ptr p) { print_string(dream_object_to_string(p)); }\n\n",
    );
}

pub(super) fn emit_iface_trampolines(out: &mut String, cx: &Cx<'_>) {
    let max_tag = cx.tags.values().copied().max().unwrap_or(12);
    let ntags = (max_tag as usize) + 1;
    for (iid, iface) in cx.mir.interfaces.interfaces.iter().enumerate() {
        for slot in 0..iface.method_count {
            let name = c_ident(&format!("__iface_dispatch_{iid}_{slot}"));
            out.push_str(&format!(
                "static void *dream_iface_{iid}_{slot}[{ntags}];\n"
            ));
            out.push_str(&format!(
                "dream_ptr {name}(dream_ptr this, dream_ptr a0, dream_ptr a1, dream_ptr a2, dream_ptr a3, dream_ptr a4, dream_ptr a5, dream_ptr a6) {{\n"
            ));
            out.push_str(&format!(
                "  int32_t tag = dream_object_tag(this);\n  dream_fn fn = (dream_fn)dream_iface_{iid}_{slot}[tag];\n  if (!fn) abort();\n  return fn(this, a0, a1, a2, a3, a4, a5, a6);\n}}\n\n"
            ));
        }
    }
}

pub(super) fn emit_iface_init(out: &mut String, cx: &Cx<'_>) {
    out.push_str("static void dream_init_itables(void) {\n");
    for imp in &cx.mir.interfaces.impls {
        let Some(tag) = interface_tag(cx, imp.class_ty) else {
            continue;
        };
        for (iid, symbols) in &imp.entries {
            for (slot, sym) in symbols.iter().enumerate() {
                let Some(f) = cx.mir.functions.iter().find(|f| f.name == *sym) else {
                    continue;
                };
                let cname = c_ident(&func_symbol(f));
                out.push_str(&format!(
                    "  dream_iface_{iid}_{slot}[{tag}] = (void *){cname};\n"
                ));
            }
        }
    }
    out.push_str("}\n\n");
}

fn interface_tag(cx: &Cx<'_>, ty: TypeId) -> Option<i32> {
    match cx.interner.kind(ty) {
        TyKind::Array(_) => Some(crate::abi::TAG_ARRAY),
        _ => cx.tags.get(&ty).copied(),
    }
}

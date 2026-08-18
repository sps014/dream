use crate::backend::c::ctx::Cx;
use crate::backend::c::rvalue::to_string_fn;
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
    }
    for layout in cx.native.unions.values() {
        let sym = format!("{}_to_string", layout.name);
        if !user.contains(&sym) {
            out.push_str(&format!("dream_ptr {}(dream_ptr p);\n", c_ident(&sym)));
        }
    }
    if !array_elems.is_empty() || !cx.native.structs.is_empty() || !cx.native.unions.is_empty() {
        out.push('\n');
    }
    for elem in array_elems {
        out.push_str(&format!(
            "dream_ptr {}(dream_ptr p) {{ return dream_array_to_string(p); }}\n",
            c_ident(&format!("array_to_string_t{}", elem.0))
        ));
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
    emit_object_to_string_router(out, cx);
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
            "  r = dream_concat_strings(r, {});\n",
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
            out.push_str(&format!("  r = dream_concat_strings(r, {value});\n"));
        } else {
            out.push_str(&format!(
                "  r = dream_concat_strings(r, {text}({value}));\n"
            ));
        }
    }
    let end = if matches!(cx.interner.kind(_ty), TyKind::Tuple(_)) {
        ")"
    } else {
        " }"
    };
    out.push_str(&format!(
        "  return dream_concat_strings(r, {});\n}}\n\n",
        cx.str_sym(end)
    ));
}

fn emit_union_to_string(out: &mut String, cx: &Cx<'_>, _ty: TypeId, name: &str) {
    let fn_name = c_ident(&format!("{name}_to_string"));
    out.push_str(&format!(
        "dream_ptr {fn_name}(dream_ptr p) {{\n  if (!p) return {};\n  return dream_object_to_string(p);\n}}\n\n",
        cx.str_sym("null")
    ));
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
        let Some(tag) = cx.tags.get(&imp.class_ty) else {
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

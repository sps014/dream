use crate::backend::c::ctx::Cx;
use crate::backend::c::types::c_ident;
use crate::backend::wasm::func_symbol;
use crate::Mir;
use dream_types::{TyKind, TypeId, TypeInterner};

pub(super) fn release_sym(interner: &TypeInterner, mir: &Mir, ty: TypeId) -> String {
    match interner.kind(ty) {
        TyKind::Struct(..) | TyKind::Union(..) => {
            if let Some(l) = mir.layouts.structs.get(&ty) {
                c_ident(&format!("release_{}", l.name))
            } else if let Some(l) = mir.layouts.unions.get(&ty) {
                c_ident(&format!("release_{}", l.name))
            } else {
                "dream_release".into()
            }
        }
        TyKind::Array(e) if interner.is_reference(*e) || interner.is_value_type(*e) => {
            c_ident(&format!("release_array_t{}", e.0))
        }
        TyKind::Func(..) => "dream_release_funcbox".into(),
        _ => "dream_release".into(),
    }
}

fn collect_array_elems(
    interner: &TypeInterner,
    f: &crate::MirFunction,
    array_elems: &mut std::collections::BTreeSet<TypeId>,
) {
    for local in &f.locals {
        if let TyKind::Array(e) = interner.kind(local.ty) {
            if interner.is_reference(*e) || interner.is_value_type(*e) {
                array_elems.insert(*e);
            }
        }
    }
}

pub(super) fn emit_release_helpers(out: &mut String, cx: &Cx<'_>) {
    let mir = cx.mir;
    let interner = cx.interner;
    let mut array_elems = std::collections::BTreeSet::new();
    for layout in cx.native.structs.values() {
        for f in &layout.fields {
            if let TyKind::Array(e) = interner.kind(f.ty) {
                if interner.is_reference(*e) || interner.is_value_type(*e) {
                    array_elems.insert(*e);
                }
            }
        }
    }
    for f in &mir.functions {
        collect_array_elems(interner, f, &mut array_elems);
        if f.is_async {
            if let Some(hir) = f.hir_fn.as_ref() {
                let body = crate::lower::lower_async_poll_body(hir, interner);
                collect_array_elems(interner, &body, &mut array_elems);
            }
        }
    }
    for elem in &array_elems {
        let name = c_ident(&format!("release_array_t{}", elem.0));
        out.push_str(&format!("static void {name}(dream_ptr p);\n"));
    }
    for layout in cx.native.structs.values() {
        let name = c_ident(&format!("release_{}", layout.name));
        out.push_str(&format!("static void {name}(dream_ptr p);\n"));
    }
    for layout in cx.native.unions.values() {
        let name = c_ident(&format!("release_{}", layout.name));
        out.push_str(&format!("static void {name}(dream_ptr p);\n"));
    }
    out.push('\n');
    for elem in array_elems {
        let name = c_ident(&format!("release_array_t{}", elem.0));
        let es = crate::backend::c::types::elem_size(cx, elem);
        out.push_str(&format!("static void {name}(dream_ptr p) {{\n"));
        out.push_str("  int32_t n; int32_t i;\n");
        out.push_str("  if (!p) return;\n");
        out.push_str("  { int32_t *rc = (int32_t *)((char *)dream_p(p) - 4); int32_t old = *rc; if (old <= 0 || old == INT32_MAX) return; *rc = old - 1; if (old != 1) return; }\n");
        out.push_str("  n = *(int32_t *)dream_p(p);\n");
        out.push_str("  for (i = 0; i < n; i++) {\n");
        if interner.is_value_type(elem) {
            let at = format!("((dream_ptr)((char *)dream_p(p) + 4 + (size_t)i * {es}))");
            crate::backend::c::statements::emit_value_refs(out, cx, elem, &at, false);
        } else if interner.is_rc_tracked(elem) {
            let rel = release_sym(interner, mir, elem);
            out.push_str(&format!(
                "    {rel}(*(dream_ptr *)((char *)dream_p(p) + 4 + (size_t)i * {es}));\n"
            ));
        }
        out.push_str("  }\n  dream_free(p);\n}\n\n");
    }
    for (ty, layout) in &cx.native.structs {
        let name = c_ident(&format!("release_{}", layout.name));
        out.push_str(&format!("static void {name}(dream_ptr p) {{\n"));
        out.push_str("  if (!p) return;\n");
        out.push_str("  { int32_t *rc = (int32_t *)((char *)dream_p(p) - 4); int32_t old = *rc; if (old <= 0 || old == INT32_MAX) return; *rc = old - 1; if (old != 1) return; }\n");
        if let Some(del) = cx
            .mir
            .functions
            .iter()
            .find(|f| f.name == format!("{}_del", layout.name))
        {
            out.push_str("  *(int32_t *)((char *)dream_p(p) - 4) = 1;\n");
            out.push_str(&format!("  {}(p);\n", c_ident(&func_symbol(del))));
        }
        for f in &layout.fields {
            if f.is_weak || f.is_unowned {
                continue;
            }
            if interner.is_value_type(f.ty) {
                let at = format!("((dream_ptr)((char *)dream_p(p) + {}))", f.offset);
                crate::backend::c::statements::emit_value_refs(out, cx, f.ty, &at, false);
            } else if interner.is_rc_tracked(f.ty) {
                let rel = release_sym(interner, mir, f.ty);
                out.push_str(&format!(
                    "  {rel}(*(dream_ptr *)((char *)dream_p(p) + {}));\n",
                    f.offset
                ));
            }
        }
        out.push_str("  dream_free(p);\n}\n\n");
        let _ = ty;
    }
    for (ty, layout) in &cx.native.unions {
        let name = c_ident(&format!("release_{}", layout.name));
        out.push_str(&format!("static void {name}(dream_ptr p) {{\n"));
        out.push_str("  if (!p) return;\n");
        out.push_str("  { int32_t *rc = (int32_t *)((char *)dream_p(p) - 4); int32_t old = *rc; if (old <= 0 || old == INT32_MAX) return; *rc = old - 1; if (old != 1) return; }\n");
        out.push_str("  switch (*(int32_t *)dream_p(p)) {\n");
        for variant in &layout.variants {
            out.push_str(&format!("    case {}:\n", variant.discriminant));
            for field in &variant.fields {
                if field.is_weak || field.is_unowned {
                    continue;
                }
                if interner.is_value_type(field.ty) {
                    let at = format!("((dream_ptr)((char *)dream_p(p) + {}))", field.offset);
                    crate::backend::c::statements::emit_value_refs(out, cx, field.ty, &at, false);
                } else if interner.is_rc_tracked(field.ty) {
                    let rel = release_sym(interner, mir, field.ty);
                    out.push_str(&format!(
                        "      {rel}(*(dream_ptr *)((char *)dream_p(p) + {}));\n",
                        field.offset
                    ));
                }
            }
            out.push_str("      break;\n");
        }
        out.push_str("    default: break;\n  }\n");
        out.push_str("  dream_free(p);\n}\n\n");
        let _ = ty;
    }
}

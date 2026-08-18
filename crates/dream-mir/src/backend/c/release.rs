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
        TyKind::Func(..) => "dream_release".into(),
        _ => "dream_release".into(),
    }
}

pub(super) fn emit_release_helpers(out: &mut String, cx: &Cx<'_>) {
    let mir = cx.mir;
    let interner = cx.interner;
    let mut array_ids = std::collections::BTreeSet::new();
    for layout in cx.native.structs.values() {
        for f in &layout.fields {
            if let TyKind::Array(e) = interner.kind(f.ty) {
                if interner.is_reference(*e) || interner.is_value_type(*e) {
                    array_ids.insert(e.0);
                }
            }
        }
    }
    for f in &mir.functions {
        for local in &f.locals {
            if let TyKind::Array(e) = interner.kind(local.ty) {
                if interner.is_reference(*e) || interner.is_value_type(*e) {
                    array_ids.insert(e.0);
                }
            }
        }
    }
    for id in &array_ids {
        let name = c_ident(&format!("release_array_t{id}"));
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
    for id in array_ids {
        let name = c_ident(&format!("release_array_t{id}"));
        out.push_str(&format!(
            "static void {name}(dream_ptr p) {{ dream_release(p); }}\n"
        ));
    }
    for (ty, layout) in &cx.native.structs {
        let name = c_ident(&format!("release_{}", layout.name));
        out.push_str(&format!("static void {name}(dream_ptr p) {{\n"));
        out.push_str("  if (!p) return;\n");
        out.push_str("  { int32_t *rc = (int32_t *)((char *)dream_p(p) - 4); int32_t old = *rc; *rc = old - 1; if (old != 1) return; }\n");
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
            if interner.is_rc_tracked(f.ty) {
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
        out.push_str("  { int32_t *rc = (int32_t *)((char *)dream_p(p) - 4); int32_t old = *rc; *rc = old - 1; if (old != 1) return; }\n");
        out.push_str("  switch (*(int32_t *)dream_p(p)) {\n");
        for variant in &layout.variants {
            out.push_str(&format!("    case {}:\n", variant.discriminant));
            for field in &variant.fields {
                if field.is_weak || field.is_unowned || !interner.is_rc_tracked(field.ty) {
                    continue;
                }
                let rel = release_sym(interner, mir, field.ty);
                out.push_str(&format!(
                    "      {rel}(*(dream_ptr *)((char *)dream_p(p) + {}));\n",
                    field.offset
                ));
            }
            out.push_str("      break;\n");
        }
        out.push_str("    default: break;\n  }\n");
        out.push_str("  dream_free(p);\n}\n\n");
        let _ = ty;
    }
}

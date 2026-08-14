//! Per-tag `$dream_drop`: nested field/element drop then `$free`.

use super::*;
use std::fmt::Write;

pub(super) fn emit_drop_glue(
    out: &mut String,
    mir: &crate::Mir,
    interner: &TypeInterner,
    tags: &HashMap<TypeId, i32>,
) {
    let mut tagged: Vec<(i32, TypeId)> = tags
        .iter()
        .filter(|(ty, _)| interner.is_reference(**ty) && !interner.is_value_type(**ty))
        .map(|(ty, tag)| (*tag, *ty))
        .collect();
    tagged.sort_by_key(|(tag, _)| *tag);

    for &(tag, ty) in &tagged {
        if let Some(layout) = mir.layouts.structs.get(&ty) {
            emit_struct_drop(out, tag, layout, interner);
        }
    }

    out.push_str("(func $dream_drop (param $ptr i32)\n (local $tag i32)\n");
    out.push_str(" local.get $ptr\n i32.eqz\n br_if 0\n");
    out.push_str(" local.get $ptr\n i32.const 12\n i32.lt_u\n br_if 0\n");
    // Immortal interned strings (size 0) must not be stamped or freed.
    out.push_str(" local.get $ptr\n i32.const 12\n i32.sub\n i32.load\n i32.eqz\n br_if 0\n");
    // reserved!=0: already freed (1) or a drop walk is in progress (2). Stamp 2 before nested
    // field/element drops so shared/cyclic heap (Regex VM `prog` aliases) cannot recurse.
    out.push_str(" local.get $ptr\n i32.const 4\n i32.sub\n i32.load\n br_if 0\n");
    out.push_str(" local.get $ptr\n i32.const 4\n i32.sub\n i32.const 2\n i32.store\n");
    out.push_str(" local.get $ptr\n call $object_tag\n local.set $tag\n");
    let _ = writeln!(
        out,
        " local.get $tag\n i32.const {}\n i32.eq\n (if (then local.get $ptr\n call $__drop_array\n return))\n",
        ARRAY_TAG
    );
    let _ = writeln!(
        out,
        " local.get $tag\n i32.const {}\n i32.eq\n (if (then local.get $ptr\n call $free\n return))\n",
        STRING_TAG
    );
    let _ = writeln!(
        out,
        " local.get $tag\n i32.const {}\n i32.eq\n (if (then local.get $ptr\n call $free\n return))\n",
        FLAT_ARRAY_TAG
    );
    for &(tag, ty) in &tagged {
        if mir.layouts.structs.contains_key(&ty) {
            let _ = writeln!(
                out,
                " local.get $tag\n i32.const {tag}\n i32.eq\n (if (then local.get $ptr\n call $__drop_t{tag}\n return))"
            );
        }
    }
    out.push_str(" local.get $ptr\n call $free\n)\n");
}

fn emit_struct_drop(
    out: &mut String,
    tag: i32,
    layout: &dream_hir::TypeLayout,
    interner: &TypeInterner,
) {
    let _ = writeln!(out, "(func $__drop_t{tag} (param $ptr i32)");
    for f in &layout.fields {
        if f.skip_nested_drop || f.is_weak {
            continue;
        }
        if interner.needs_drop(f.ty) {
            out.push_str(" local.get $ptr\n");
            if f.offset > 0 {
                let _ = writeln!(out, " i32.const {}\n i32.add", f.offset);
            }
            out.push_str(" i32.load\n call $dream_drop\n");
        }
    }
    out.push_str(" local.get $ptr\n call $free\n)\n");
}

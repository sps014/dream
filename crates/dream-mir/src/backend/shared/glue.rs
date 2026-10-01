//! ARC glue policy: which `release_*` / `destroy_*` symbol a type uses, which types share one
//! body, what each field's last-drop teardown is, and which array element types need glue.
//! The IR writers only decide *how* to print the bodies.

use super::abi_types::c_ident;
use super::cx::Cx;
use super::symbols::func_symbol;
use dream_hir::TypeLayout;
use dream_types::{TyKind, TypeId, TypeInterner};
use indexmap::IndexMap as HashMap;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn retain_sym(cx: &Cx<'_>, ty: TypeId) -> &'static str {
    if cx.target.is_wasm32() && matches!(cx.interner.kind(ty), TyKind::Js) {
        "js_retain"
    } else {
        "dream_retain"
    }
}

/// For a niche union, the release/destroy symbol of its single reference payload. A niche
/// envelope *is* the payload pointer, so ARC ops must run the payload's own cascade (a flat
/// `dream_release` would free a class node without tearing down its fields). All generated
/// `release_*`/`destroy_*` bodies start with their own null check, so forwarding is safe.
pub(crate) fn niche_payload_glue(cx: &Cx<'_>, ty: TypeId, destroy: bool) -> Option<String> {
    let field = niche_payload_ty(cx, ty)?;
    Some(if destroy {
        destroy_sym(cx, field)
    } else {
        release_sym(cx, field)
    })
}

fn niche_payload_ty(cx: &Cx<'_>, ty: TypeId) -> Option<TypeId> {
    if !cx.interner.is_niche_union(ty) {
        return None;
    }
    let u = cx.mir.layouts.unions.get(&ty)?;
    Some(
        u.variants
            .iter()
            .find(|v| !v.fields.is_empty())?
            .fields
            .first()?
            .ty,
    )
}

/// The split-tail variant of a locally-generated release glue symbol, when one exists.
/// `Statement::Release` inlines the null-check + decrement at the call site and calls this
/// tail only on the free transition, so the common not-last case is call-free.
pub(crate) fn release_into_sym(cx: &Cx<'_>, ty: TypeId) -> Option<String> {
    // Niche unions forward to their payload's glue (recursion terminates: payloads shrink).
    if cx.interner.is_niche_union(ty) {
        return release_into_sym(cx, niche_payload_ty(cx, ty)?);
    }
    if !cx.interner.is_rc_tracked(ty) {
        return None;
    }
    let (raw, map_key) = match cx.interner.kind(ty) {
        TyKind::Struct(..) => (
            c_ident(&format!(
                "release_{}",
                cx.mir.layouts.structs.get(&ty)?.name
            )),
            ty,
        ),
        TyKind::Union(..) => (
            c_ident(&format!("release_{}", cx.mir.layouts.unions.get(&ty)?.name)),
            ty,
        ),
        TyKind::Array(e) if cx.interner.is_reference(*e) || cx.interner.is_value_type(*e) => {
            // Array glue is never in `canon_maps` (those keys are struct/union ids). Looking
            // up the element id would steal a redirected *element* `release_Foo` and drop the
            // array header as if it were a `Foo` — SIGSEGV when two classes share a layout
            // (e.g. `Job` and `CancelledError`) and `Option.unwrap` temps a `T[]`.
            return Some(format!(
                "{}_into",
                c_ident(&format!("release_array_t{}", e.0))
            ));
        }
        _ => return None,
    };
    // Follow canonicalization to the representative body's tail.
    let final_sym = cx
        .canon_maps()
        .release
        .get(&map_key)
        .cloned()
        .unwrap_or(raw);
    Some(format!("{final_sym}_into"))
}

pub(crate) fn release_sym(cx: &Cx<'_>, ty: TypeId) -> String {
    let interner = cx.interner;
    let mir = cx.mir;
    // A niche union *is* its payload pointer — run the payload's release cascade.
    if let Some(sym) = niche_payload_glue(cx, ty, false) {
        return sym;
    }
    match interner.kind(ty) {
        TyKind::Js if cx.target.is_wasm32() => "js_release".into(),
        TyKind::Struct(..) | TyKind::Union(..) => {
            let raw = if let Some(l) = mir.layouts.structs.get(&ty) {
                c_ident(&format!("release_{}", l.name))
            } else if let Some(l) = mir.layouts.unions.get(&ty) {
                c_ident(&format!("release_{}", l.name))
            } else {
                return "dream_release".into();
            };
            cx.canon_maps().release.get(&ty).cloned().unwrap_or(raw)
        }
        TyKind::Array(e) if interner.is_reference(*e) || interner.is_value_type(*e) => {
            c_ident(&format!("release_array_t{}", e.0))
        }
        TyKind::Func(..) => "dream_release_funcbox".into(),
        TyKind::Prim(dream_types::PrimTy::String) if mir.uses_defer => "release_string".into(),
        // The static type says nothing about the referent's layout, so the runtime tag has to
        // pick the cascade — a flat `dream_release` recycles the block and strands its fields.
        // `destroy_sym` already dispatches this way.
        TyKind::Object | TyKind::Interface(..) => c_ident("dream_release_object"),
        _ => "dream_release".into(),
    }
}

pub(crate) fn destroy_sym(cx: &Cx<'_>, ty: TypeId) -> String {
    // A niche union *is* its payload pointer — destroying it uniquely destroys the payload.
    if let Some(sym) = niche_payload_glue(cx, ty, true) {
        return sym;
    }
    if matches!(cx.interner.kind(ty), TyKind::Js | TyKind::Func(..))
        || cx.interner.is_shared_type(ty)
        || matches!(
            cx.interner.kind(ty),
            TyKind::Prim(dream_types::PrimTy::String)
        )
    {
        return release_sym(cx, ty);
    }
    match cx.interner.kind(ty) {
        TyKind::Struct(..) | TyKind::Union(..) => {
            let raw = if let Some(l) = cx.mir.layouts.structs.get(&ty) {
                c_ident(&format!("destroy_{}", l.name))
            } else if let Some(l) = cx.mir.layouts.unions.get(&ty) {
                c_ident(&format!("destroy_{}", l.name))
            } else {
                return c_ident("destroy_object");
            };
            cx.canon_maps().destroy.get(&ty).cloned().unwrap_or(raw)
        }
        TyKind::Array(e) if cx.interner.is_reference(*e) || cx.interner.is_value_type(*e) => {
            c_ident(&format!("destroy_array_t{}", e.0))
        }
        TyKind::Object | TyKind::Interface(..) => c_ident("destroy_object"),
        _ => "dream_destroy".into(),
    }
}

/// Symbol canonicalization for ARC glue: types whose `release_*` (resp.
/// `destroy_*`) bodies would be byte-identical share one emitted function —
/// fieldless structs and same-shape classes. Maps contain only *redirects*:
/// entries whose target differs from the type's own raw symbol, keyed by
/// struct/union id. Array helpers are never redirected through this map (an
/// element TypeId may already be a struct redirect).
pub(crate) struct CanonMaps {
    pub release: HashMap<TypeId, String>,
    pub destroy: HashMap<TypeId, String>,
}

pub(crate) fn canonical_maps(cx: &Cx<'_>) -> CanonMaps {
    let mut rel: Vec<(String, TypeId, String)> = Vec::new();
    let mut des: Vec<(String, TypeId, String)> = Vec::new();
    for (ty, layout) in &cx.native.structs {
        if layout.has_destructor() {
            continue;
        }
        let key = struct_profile_key(cx, layout);
        rel.push((
            format!("S|rel|{key}"),
            *ty,
            c_ident(&format!("release_{}", layout.name)),
        ));
        des.push((
            format!("S|des|{key}"),
            *ty,
            c_ident(&format!("destroy_{}", layout.name)),
        ));
    }
    for (ty, layout) in &cx.native.unions {
        let key = union_profile_key(cx, layout);
        rel.push((
            format!("U|rel|{key}"),
            *ty,
            c_ident(&format!("release_{}", layout.name)),
        ));
        des.push((
            format!("U|des|{key}"),
            *ty,
            c_ident(&format!("destroy_{}", layout.name)),
        ));
    }
    // Arrays are excluded from dedup: their bodies embed the element type's own
    // release symbol, so identical-looking shapes (a plain ref union vs string)
    // still differ. The empty-loop skip already minimizes the trivial ones.
    CanonMaps {
        release: redirects(rel),
        destroy: redirects(des),
    }
}

/// Group candidates by body-shape key and pick the lexically smallest symbol in
/// each group as the representative (BTreeMap + sort keep output deterministic).
fn redirects(cands: Vec<(String, TypeId, String)>) -> HashMap<TypeId, String> {
    let mut groups: BTreeMap<String, Vec<(TypeId, String)>> = BTreeMap::new();
    for (key, ty, sym) in cands {
        groups.entry(key).or_default().push((ty, sym));
    }
    let mut out = HashMap::new();
    for (_, mut members) in groups {
        members.sort_by(|a, b| a.1.cmp(&b.1));
        let rep = members[0].1.clone();
        for (ty, sym) in members {
            if sym != rep {
                out.insert(ty, rep.clone());
            }
        }
    }
    out
}

/// The `del` symbol a type's last drop calls (after reviving the object), when it has one.
pub(crate) fn del_symbol(cx: &Cx<'_>, def: dream_types::DefId) -> String {
    let function = cx
        .mir
        .functions
        .iter()
        .find(|f| f.def == def && f.instance.is_empty())
        .unwrap_or_else(|| crate::internal_error!("resolved destructor missing from MIR"));
    c_ident(&func_symbol(function))
}

fn field_flag(cx: &Cx<'_>, f: &dream_hir::FieldLayout) -> &'static str {
    if f.is_weak {
        "w"
    } else if f.is_unowned {
        "u"
    } else if cx.interner.is_value_type(f.ty) {
        "v"
    } else if cx.interner.is_rc_tracked(f.ty) {
        "r"
    } else {
        ""
    }
}

/// Body key for a struct's glue: everything the emitted code can depend on
/// (field offsets, types, ownership flags).
fn struct_profile_key(cx: &Cx<'_>, layout: &TypeLayout) -> String {
    let mut key = String::new();
    for f in &layout.fields {
        key.push_str(&format!("|{}:{}{}", f.offset, f.ty.0, field_flag(cx, f)));
    }
    key
}

fn union_profile_key(cx: &Cx<'_>, layout: &dream_hir::UnionLayout) -> String {
    let mut key = String::new();
    for v in &layout.variants {
        key.push_str(&format!("|d{}", v.discriminant));
        for f in &v.fields {
            key.push_str(&format!(",{}:{}{}", f.offset, f.ty.0, field_flag(cx, f)));
        }
    }
    key
}

/// One field's teardown on its holder's last drop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FieldDrop {
    None,
    /// Unowned slots live in the weak registry (registered on store); destroying the holder
    /// must unregister them or a later clear of the target writes into freed memory.
    Unregister {
        offset: u32,
    },
    /// An inline value struct/union: walk its own reference fields.
    Value {
        offset: u32,
        ty: TypeId,
    },
    /// A strong reference: `destroy_*` when uniquely owned, else `release_*`.
    Rc {
        offset: u32,
        ty: TypeId,
    },
}

pub(crate) fn field_drop(cx: &Cx<'_>, f: &dream_hir::FieldLayout, in_union: bool) -> FieldDrop {
    if f.is_weak || (in_union && f.is_unowned) {
        return FieldDrop::None;
    }
    if f.is_unowned {
        return FieldDrop::Unregister { offset: f.offset };
    }
    if cx.interner.is_value_type(f.ty) {
        FieldDrop::Value {
            offset: f.offset,
            ty: f.ty,
        }
    } else if cx.interner.is_rc_tracked(f.ty) {
        FieldDrop::Rc {
            offset: f.offset,
            ty: f.ty,
        }
    } else {
        FieldDrop::None
    }
}

pub(crate) fn struct_field_drops(cx: &Cx<'_>, layout: &TypeLayout) -> Vec<FieldDrop> {
    layout
        .fields
        .iter()
        .map(|f| field_drop(cx, f, false))
        .collect()
}

/// Whether walking an inline value of `ty` (retain or release of its reference fields) emits
/// any code. Union walks always switch on the tag, so they count as non-empty.
pub(crate) fn value_walk_nonempty(cx: &Cx<'_>, ty: TypeId) -> bool {
    if let Some(layout) = cx.nstruct(ty) {
        return layout.fields.iter().any(|f| {
            !f.is_weak
                && !f.is_unowned
                && if cx.interner.is_value_type(f.ty) {
                    value_walk_nonempty(cx, f.ty)
                } else {
                    cx.interner.is_rc_tracked(f.ty)
                }
        });
    }
    cx.nunion(ty).is_some()
}

/// Whether field teardown `d` emits any code.
pub(crate) fn drop_nonempty(cx: &Cx<'_>, d: FieldDrop) -> bool {
    match d {
        FieldDrop::None => false,
        FieldDrop::Value { ty, .. } => value_walk_nonempty(cx, ty),
        FieldDrop::Unregister { .. } | FieldDrop::Rc { .. } => true,
    }
}

/// The field a destroy loop can continue into: the last field with any teardown, when it is
/// a strong reference whose unique destroy is `destroy` itself. Types with `del` keep plain
/// recursion so a child's `del` still runs before its parent's block is recycled.
/// `has_teardown[i]` says whether field `i` emits anything (value fields can be empty).
pub(crate) fn self_tail_field(
    cx: &Cx<'_>,
    layout: &TypeLayout,
    has_teardown: &[bool],
    destroy: &str,
) -> Option<usize> {
    if layout.has_destructor() {
        return None;
    }
    let i = has_teardown.iter().rposition(|d| *d)?;
    let f = &layout.fields[i];
    let self_typed = !f.is_unowned
        && !cx.interner.is_value_type(f.ty)
        && cx.interner.is_rc_tracked(f.ty)
        && destroy_sym(cx, f.ty) == destroy
        && release_sym(cx, f.ty) != destroy;
    self_typed.then_some(i)
}

fn collect_array_elems(
    interner: &TypeInterner,
    f: &crate::MirFunction,
    array_elems: &mut BTreeSet<TypeId>,
) {
    for local in &f.locals {
        if let TyKind::Array(e) = interner.kind(local.ty) {
            if interner.is_reference(*e) || interner.is_value_type(*e) {
                array_elems.insert(*e);
            }
        }
    }
}

/// Element types whose `release_array_t{N}` / `destroy_array_t{N}` glue the module needs.
pub(crate) fn glue_array_elems(cx: &Cx<'_>) -> BTreeSet<TypeId> {
    let interner = cx.interner;
    let mut array_elems = BTreeSet::new();
    for layout in cx.native.structs.values() {
        for f in &layout.fields {
            if let TyKind::Array(e) = interner.kind(f.ty) {
                if interner.is_reference(*e) || interner.is_value_type(*e) {
                    array_elems.insert(*e);
                }
            }
        }
    }
    for f in &cx.mir.functions {
        collect_array_elems(interner, f, &mut array_elems);
    }
    for p in &cx.mir.polls {
        collect_array_elems(interner, p, &mut array_elems);
    }
    // Funcbox last-drop of `TAG_CLOSURE_ENV` always forwards to `release_array_t{object}`.
    array_elems.insert(interner.object());
    array_elems
}

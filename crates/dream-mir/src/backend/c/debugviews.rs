//! Debugger-only typed views over raw locals (`-g`, native targets).
//!
//! Reference-typed locals are `dream_ptr` integers in the emitted C, so DWARF shows scalars.
//! For every source-named local whose type carries runtime structure, an extra declaration
//! `const T *const __v_<name>` points at the same bytes through a C struct shaped exactly like
//! the real layout: class instances expand into their fields, strings into `{len, units}`,
//! unions into `{tag, variant fields}`. Views are pure declarations — no instructions, no
//! behavior change, absent from release output.

use super::ast::{CTy, Expr, Item, Stmt};
use super::ctx::Cx;
use super::types::c_ident;
use crate::MirFunction;
use dream_hir::UnionLayout;
use dream_types::{PrimTy, TyKind, TypeId};
use indexmap::IndexSet;

pub(super) fn enabled(cx: &Cx<'_>) -> bool {
    cx.debug_syms && !cx.target.is_wasm32()
}

/// The alias spelling for a structured heap type, consistent between module-level `typedef`s and
/// every reference to them. `None` for types without a named runtime shape.
fn alias_for(cx: &Cx<'_>, ty: TypeId) -> Option<String> {
    let base = match cx.interner.kind(ty) {
        TyKind::Prim(PrimTy::String) => "dream_Str".to_string(),
        TyKind::Struct(_, _) => c_ident(&cx.nstruct(ty)?.name),
        TyKind::Union(_, _) => c_ident(&cx.nunion(ty)?.name),
        _ => return None,
    };
    let args: &[TypeId] = match cx.interner.kind(ty) {
        TyKind::Struct(_, args) | TyKind::Union(_, args) => args,
        _ => &[],
    };
    if args.is_empty() {
        return Some(base);
    }
    let ids: Vec<String> = args.iter().map(|a| a.0.to_string()).collect();
    Some(format!("{base}_{}", ids.join("_")))
}

fn str_view_ty() -> CTy {
    CTy::Struct {
        fields: vec![
            (CTy::I32, "len".into()),
            (CTy::I32, "_pad".into()),
            (
                CTy::Array {
                    elem: Box::new(CTy::U16),
                    len: 0,
                },
                "units".into(),
            ),
        ],
    }
}

/// Module-level `typedef`s backing the views: the string shape plus every nominal struct and
/// discriminated union in the module.
/// View aliases reference each other through pointer fields, so every name gets a forward
/// declaration up front and definitions follow in any order.
pub(super) fn module_alias_items(cx: &Cx<'_>) -> Vec<Item> {
    if !enabled(cx) {
        return vec![];
    }
    let mut defs: Vec<(String, CTy, bool)> = vec![(
        "dream_Str".into(),
        str_view_ty(),
        false,
    )];
    let push_def = |defs: &mut Vec<(String, CTy, bool)>, ty: TypeId| {
        let Some(name) = alias_for(cx, ty) else {
            return;
        };
        let fields = match cx.interner.kind(ty) {
            TyKind::Struct(_, _) => padded_fields(
                cx,
                cx.nstruct(ty)
                    .unwrap()
                    .fields
                    .iter()
                    .map(|f| (f.offset, f.ty, f.name.clone())),
            ),
            TyKind::Union(_, _) => union_fields(cx, cx.nunion(ty).unwrap()),
            _ => return,
        };
        defs.push((name, CTy::Struct { fields }, false));
    };
    for ty in cx.native.structs.keys() {
        push_def(&mut defs, *ty);
    }
    for ty in cx.native.unions.keys() {
        push_def(&mut defs, *ty);
    }
    // One packed view per element type that actually appears inside an array: elements sit
    // unaligned at offset 4, so only a packed struct can type them truthfully.
    let mut arr_elems: IndexSet<TypeId> = IndexSet::new();
    for f in cx.mir.functions.iter() {
        for d in f.locals.iter() {
            collect_array_elems(cx, d.ty, &mut arr_elems);
        }
    }
    for layout in cx.native.structs.values() {
        for f in &layout.fields {
            collect_array_elems(cx, f.ty, &mut arr_elems);
        }
    }
    for e in &arr_elems {
        defs.push((
            arr_alias(cx, *e),
            CTy::Struct {
                fields: vec![
                    (CTy::I32, "len".into()),
                    (
                        CTy::Array {
                            elem: Box::new(field_c_ty(cx, *e)),
                            len: 0,
                        },
                        "elems".into(),
                    ),
                ],
            },
            true,
        ));
    }
    let mut items: Vec<Item> = Vec::with_capacity(defs.len() * 2);
    for (name, _, _) in &defs {
        items.push(Item::AliasFwd {
            tag: alias_tag(name),
            name: name.clone(),
        });
    }
    for (name, ty, packed) in defs {
        items.push(Item::Alias {
            tag: alias_tag(&name),
            name,
            ty,
            packed,
        });
    }
    items
}

/// The struct tag backing a view alias (`Point` -> `__dv_Point`).
fn alias_tag(name: &str) -> String {
    format!("__dv_{name}")
}

/// Fields with explicit padding so the C struct lands byte-for-byte on the runtime layout.
/// `(offset, type, name)` triples in increasing-offset order.
fn padded_fields(
    cx: &Cx<'_>,
    fields: impl Iterator<Item = (u32, TypeId, String)>,
) -> Vec<(CTy, String)> {
    let mut out: Vec<(CTy, String)> = Vec::new();
    let mut cursor = 0u32;
    let mut pads = 0u32;
    for (off, fty, name) in fields {
        if off > cursor {
            pads += 1;
            out.push((
                CTy::Array {
                    elem: Box::new(CTy::Named("unsigned char")),
                    len: (off - cursor) as usize,
                },
                format!("_pad{pads}"),
            ));
        }
        out.push((field_c_ty(cx, fty), name));
        cursor = off + super::types::native_scalar_size(cx, fty).0.max(1);
    }
    out
}

/// Discriminated unions share one block: the discriminant word at offset 0 identifies the active
/// variant; every variant's payload sits at its fixed block offset, so a flat struct exposes all
/// of them with `<Variant>_`-prefixed names (the active variant's are the meaningful ones).
fn union_fields(cx: &Cx<'_>, u: &UnionLayout) -> Vec<(CTy, String)> {
    let variants: Vec<(CTy, String)> = u
        .variants
        .iter()
        .map(|var| {
            let fields: Vec<(u32, TypeId, String)> = var
                .fields
                .iter()
                .filter(|f| f.offset >= 4 && f.offset < u.size)
                .map(|f| (f.offset - 4, f.ty, f.name.clone()))
                .collect();
            (
                CTy::Struct {
                    fields: padded_fields(cx, fields.into_iter()),
                },
                var.name.clone(),
            )
        })
        .collect();
    vec![
        (CTy::I32, "tag".into()),
        (CTy::Union { fields: variants }, "value".into()),
    ]
}

/// Recursively records every element type reachable through array shapes at `ty`.
fn collect_array_elems(cx: &Cx<'_>, ty: TypeId, out: &mut IndexSet<TypeId>) {
    match cx.interner.kind(ty) {
        TyKind::Array(elem) => {
            out.insert(*elem);
            collect_array_elems(cx, *elem, out);
        }
        TyKind::Struct(_, _) => {
            if let Some(l) = cx.nstruct(ty) {
                for f in &l.fields {
                    collect_array_elems(cx, f.ty, out);
                }
            }
        }
        _ => {}
    }
}

/// Readable element spelling for typed array-view aliases (`dream_Arr_int`, `dream_Arr_Point`).
fn elem_tag(cx: &Cx<'_>, ty: TypeId) -> String {
    match cx.interner.kind(ty) {
        TyKind::Prim(p) => p.name().to_string(),
        TyKind::Struct(_, _) => c_ident(&cx.nstruct(ty).unwrap().name),
        TyKind::Union(_, _) => c_ident(&cx.nunion(ty).unwrap().name),
        TyKind::Enum(_) => format!("e{}", ty.0),
        TyKind::Array(inner) => format!("arr_{}", elem_tag(cx, *inner)),
        TyKind::Tuple(elems) => {
            let ids: Vec<String> = elems.iter().map(|t| elem_tag(cx, *t)).collect();
            format!("tuple_{}", ids.join("_"))
        }
        _ => format!("t{}", ty.0),
    }
}

fn arr_alias(cx: &Cx<'_>, elem: TypeId) -> String {
    format!("dream_Arr_{}", elem_tag(cx, elem))
}

/// How a field of Dream type `fty` renders inside a view: scalars directly, structured heap
/// references as pointers to their alias.
fn field_c_ty(cx: &Cx<'_>, fty: TypeId) -> CTy {
    if let TyKind::Array(elem) = cx.interner.kind(fty) {
        return CTy::ptr_to(CTy::Ident(arr_alias(cx, *elem)));
    }
    if let Some(alias) = alias_for(cx, fty) {
        return CTy::ptr_to(CTy::Ident(alias));
    }
    super::types::c_ty(cx.interner, fty)
}

/// One view declaration per source-named local whose type carries runtime structure. Heap locals
/// view their own pointer word; non-parameter value locals view their `__vs{i}` storage buffer.
pub(super) fn local_debug_views(cx: &Cx<'_>, f: &MirFunction) -> Vec<Stmt> {
    if !enabled(cx) {
        return vec![];
    }
    let mut out = Vec::new();
    for (i, decl) in f.locals.iter().enumerate() {
        let Some(name) = decl.name.as_deref() else {
            continue;
        };
        let is_param = f.params.iter().any(|p| p.0 == i as u32);
        let kind = cx.interner.kind(decl.ty);
        let is_value = cx.interner.is_value_type(decl.ty);
        let pointee: Option<CTy> = match kind {
            TyKind::Prim(PrimTy::String) | TyKind::Struct(_, _) | TyKind::Union(_, _) => alias_for(
                cx,
                decl.ty,
            )
            .map(|alias| CTy::ptr_to(CTy::Ident(alias))),
            TyKind::Array(elem) => {
                Some(CTy::ptr_to(CTy::Ident(arr_alias(cx, *elem))))
            }
            _ if is_value && !is_param => match kind {
                TyKind::Struct(_, _) => alias_for(cx, decl.ty).map(|a| CTy::ptr_to(CTy::Ident(a))),
                TyKind::Tuple(elems) => Some(CTy::ptr_to(CTy::Struct {
                    fields: tuple_fields(cx, elems),
                })),
                _ => None,
            },
            _ => None,
        };
        let Some(pointee) = pointee else {
            continue;
        };
        // A local cannot be both a heap reference and inline storage; pick the right base.
        //
        // Heap locals are integers that get reassigned, so the view must point at the local's
        // *slot* (`T *const *`) to stay live; value-local storage never moves, so a direct
        // pointer into the `__vs` buffer is enough.
        let (ty, init) = if is_value {
            debug_assert!(!is_param);
            let init = Expr::cast(
                pointee.clone(),
                Expr::addr_of(Expr::id(format!("__vs{i}"))),
            );
            (pointee, init)
        } else {
            let slot_ty = CTy::ptr_to(pointee.clone());
            let init = Expr::cast(
                slot_ty.clone(),
                Expr::addr_of(Expr::local(i as u32)),
            );
            (slot_ty, init)
        };
        out.push(Stmt::Decl {
            align: None,
            static_: false,
            const_: true,
            ty,
            name: format!("__v_{name}"),
            init: Some(init),
        });
    }
    out
}

/// Tuple elements inline, aligned like `TypeLayout::from_fields`.
fn tuple_fields(cx: &Cx<'_>, elems: &[TypeId]) -> Vec<(CTy, String)> {
    let mut out: Vec<(CTy, String)> = Vec::new();
    let mut cursor = 0u32;
    let mut pads = 0u32;
    for (i, e) in elems.iter().enumerate() {
        let (sz, align) = super::types::native_scalar_size(cx, *e);
        let off = (cursor + align - 1) & !(align - 1);
        if off > cursor {
            pads += 1;
            out.push((
                CTy::Array {
                    elem: Box::new(CTy::Named("unsigned char")),
                    len: (off - cursor) as usize,
                },
                format!("_pad{pads}"),
            ));
        }
        out.push((field_c_ty(cx, *e), format!("t{i}")));
        cursor = off + sz.max(1);
    }
    out
}

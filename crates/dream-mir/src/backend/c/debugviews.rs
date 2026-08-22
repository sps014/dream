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
pub(super) fn module_alias_items(cx: &Cx<'_>) -> Vec<Item> {
    if !enabled(cx) {
        return vec![];
    }
    let mut items = vec![
        Item::Alias {
            name: "dream_Str".into(),
            ty: str_view_ty(),
        },
        // Arrays are `[count:i32][elems...]`; elements are exposed as raw bytes because the
        // runtime packs them at offset 4 with no C-representable alignment guarantee.
        Item::Alias {
            name: "dream_Arr".into(),
            ty: CTy::Struct {
                fields: vec![
                    (CTy::I32, "len".into()),
                    (
                        CTy::Array {
                            elem: Box::new(CTy::Named("unsigned char")),
                            len: 0,
                        },
                        "bytes".into(),
                    ),
                ],
            },
        },
    ];
    let push_alias = |items: &mut Vec<Item>, ty: TypeId| {
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
        items.push(Item::Alias {
            name,
            ty: CTy::Struct { fields },
        });
    };
    for ty in cx.native.structs.keys() {
        push_alias(&mut items, *ty);
    }
    for ty in cx.native.unions.keys() {
        push_alias(&mut items, *ty);
    }
    items
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
    let mut fields: Vec<(u32, TypeId, String)> = Vec::new();
    for var in &u.variants {
        for f in &var.fields {
            if f.offset < 4 || f.offset >= u.size {
                continue;
            }
            fields.push((f.offset, f.ty, format!("{}_{}", var.name, f.name)));
        }
    }
    let mut out = padded_fields(cx, fields.into_iter());
    out.insert(0, (CTy::I32, "tag".into()));
    out
}

/// How a field of Dream type `fty` renders inside a view: scalars directly, structured heap
/// references as pointers to their alias.
fn field_c_ty(cx: &Cx<'_>, fty: TypeId) -> CTy {
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
            TyKind::Array(_) => Some(CTy::ptr_to(CTy::Named("dream_Arr"))),
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

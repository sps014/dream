//! Host pointer width (8) layouts. MIR/HIR layouts assume WASM i32 pointers.

use crate::Mir;
use dream_hir::{FieldLayout, TypeLayout, UnionLayout, UnionVariant};
use dream_types::{TyKind, TypeId, TypeInterner};
use indexmap::IndexMap;

pub(super) struct NativeLayouts {
    pub structs: IndexMap<TypeId, TypeLayout>,
    pub unions: IndexMap<TypeId, UnionLayout>,
}

impl NativeLayouts {
    pub(super) fn compute(mir: &Mir, interner: &TypeInterner) -> Self {
        let mut structs = IndexMap::new();
        let mut pending: Vec<(TypeId, TypeLayout)> = mir
            .layouts
            .structs
            .iter()
            .map(|(ty, l)| (*ty, l.clone()))
            .collect();
        loop {
            if pending.is_empty() {
                break;
            }
            let mut rest = Vec::new();
            let mut progress = false;
            for (ty, wasm) in pending {
                match relayout_struct(interner, &structs, &wasm) {
                    Some(native) => {
                        structs.insert(ty, native);
                        progress = true;
                    }
                    None => rest.push((ty, wasm)),
                }
            }
            if rest.is_empty() {
                break;
            }
            if !progress {
                for (ty, wasm) in rest {
                    structs.insert(ty, force_struct(interner, &structs, &wasm));
                }
                break;
            }
            pending = rest;
        }
        let mut unions = IndexMap::new();
        for (ty, wasm) in &mir.layouts.unions {
            unions.insert(*ty, relayout_union(interner, &structs, wasm));
        }
        Self { structs, unions }
    }
}

fn native_scalar(
    interner: &TypeInterner,
    structs: &IndexMap<TypeId, TypeLayout>,
    ty: TypeId,
) -> Option<(u32, u32)> {
    if interner.is_value_type(ty) {
        if let Some(l) = structs.get(&ty) {
            let align = l.fields.iter().fold(4u32, |a, f| {
                native_scalar(interner, structs, f.ty)
                    .map(|(_, al)| a.max(al))
                    .unwrap_or(a)
            });
            return Some((l.size, align.max(1)));
        }
        return None;
    }
    match interner.kind(ty) {
        TyKind::Prim(dream_types::PrimTy::String) => Some((8, 8)),
        TyKind::Prim(p) => Some(p.size_align()),
        TyKind::Enum(_) => Some((4, 4)),
        _ => Some((8, 8)),
    }
}

fn relayout_struct(
    interner: &TypeInterner,
    structs: &IndexMap<TypeId, TypeLayout>,
    wasm: &TypeLayout,
) -> Option<TypeLayout> {
    if wasm.packed {
        return pack_struct(interner, structs, wasm);
    }
    let mut offset = 0u32;
    let mut max_align = 4u32;
    let mut fields = Vec::with_capacity(wasm.fields.len());
    for f in &wasm.fields {
        let (size, align) = native_scalar(interner, structs, f.ty)?;
        offset = align_up(offset, align);
        fields.push(FieldLayout {
            offset,
            ty: f.ty,
            name: f.name.clone(),
            is_weak: f.is_weak,
            is_unowned: f.is_unowned,
        });
        offset += size;
        max_align = max_align.max(align);
    }
    Some(TypeLayout {
        name: wasm.name.clone(),
        fields,
        size: align_up(offset, max_align),
        packed: false,
    })
}

fn pack_struct(
    interner: &TypeInterner,
    structs: &IndexMap<TypeId, TypeLayout>,
    wasm: &TypeLayout,
) -> Option<TypeLayout> {
    let mut offset = 0u32;
    let mut fields = Vec::with_capacity(wasm.fields.len());
    for f in &wasm.fields {
        let (size, _) = native_scalar(interner, structs, f.ty)?;
        fields.push(FieldLayout {
            offset,
            ty: f.ty,
            name: f.name.clone(),
            is_weak: f.is_weak,
            is_unowned: f.is_unowned,
        });
        offset += size;
    }
    Some(TypeLayout {
        name: wasm.name.clone(),
        fields,
        size: offset,
        packed: true,
    })
}

fn force_struct(
    interner: &TypeInterner,
    structs: &IndexMap<TypeId, TypeLayout>,
    wasm: &TypeLayout,
) -> TypeLayout {
    relayout_struct(interner, structs, wasm).unwrap_or_else(|| wasm.clone())
}

fn relayout_union(
    interner: &TypeInterner,
    structs: &IndexMap<TypeId, TypeLayout>,
    wasm: &UnionLayout,
) -> UnionLayout {
    let mut variants = Vec::with_capacity(wasm.variants.len());
    let mut size = 4u32;
    for v in &wasm.variants {
        let mut offset = 4u32;
        let mut fields = Vec::with_capacity(v.fields.len());
        for f in &v.fields {
            let (fsz, align) = native_scalar(interner, structs, f.ty).unwrap_or((8, 8));
            offset = align_up(offset, align);
            fields.push(FieldLayout {
                offset,
                ty: f.ty,
                name: f.name.clone(),
                is_weak: f.is_weak,
                is_unowned: f.is_unowned,
            });
            offset += fsz;
        }
        size = size.max(offset);
        variants.push(UnionVariant {
            name: v.name.clone(),
            discriminant: v.discriminant,
            fields,
        });
    }
    UnionLayout {
        name: wasm.name.clone(),
        variants,
        size,
    }
}

fn align_up(offset: u32, align: u32) -> u32 {
    let rem = offset % align;
    if rem == 0 {
        offset
    } else {
        offset + (align - rem)
    }
}

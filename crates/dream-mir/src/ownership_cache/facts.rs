//! What a function's ownership lowering reads about the types and globals it names directly:
//! their layouts and value-ness, closed over field and component types.

use super::ids::{Ids, hash};
use crate::Mir;
use dream_hir::{FieldLayout, LayoutTable};
use dream_types::portable::{IdSpace, IdTables, StableIds};
use dream_types::{DefId, TyKind, TypeId, TypeInterner};
use indexmap::{IndexMap, IndexSet};

pub(super) struct Facts {
    /// Each type's own facts with its stable identity, or `None` when something it names has
    /// no stable identity.
    types: Vec<Option<(u128, u128)>>,
    interfaces: Vec<TypeId>,
    globals: IndexMap<u64, TypeId>,
}

impl Facts {
    pub(super) fn new(mir: &Mir, interner: &TypeInterner, ids: &Ids) -> Self {
        use rayon::prelude::*;
        let types = (0..interner.len() as u32)
            .into_par_iter()
            .map(|raw| {
                let ty = TypeId(raw);
                Some((ids.ty(ty)?, type_facts(ty, interner, &mir.layouts, ids)?))
            })
            .collect();
        Self {
            types,
            interfaces: mir.interfaces.interfaces.iter().map(|info| info.ty).collect(),
            globals: mir
                .globals
                .iter()
                .map(|global| (global.id.0 as u64, global.ty))
                .collect(),
        }
    }

    /// Digest of the facts behind every id in `used`, which [`portable::encode_stable`] returned
    /// for the function.
    ///
    /// [`portable::encode_stable`]: dream_types::portable::encode_stable
    pub(super) fn digest(
        &self,
        used: &IdTables,
        interner: &TypeInterner,
        layouts: &LayoutTable,
        ids: &Ids,
    ) -> Option<u128> {
        let mut closure: IndexSet<TypeId> = IndexSet::new();
        let mut pending: Vec<TypeId> = used
            .get(IdSpace::Type)
            .iter()
            .map(|&raw| TypeId(raw as u32))
            .collect();
        for &raw in used.get(IdSpace::Interface) {
            pending.push(*self.interfaces.get(raw as usize)?);
        }
        let mut globals = Vec::new();
        for &raw in used.get(IdSpace::Global) {
            let ty = *self.globals.get(&raw)?;
            pending.push(ty);
            globals.push((ids.stable(IdSpace::Global, raw)?, ids.ty(ty)?));
        }
        while let Some(ty) = pending.pop() {
            if closure.insert(ty) {
                pending.extend(children(ty, interner, layouts));
            }
        }
        let mut types = closure
            .iter()
            .map(|ty| *self.types.get(ty.0 as usize)?)
            .collect::<Option<Vec<_>>>()?;
        types.sort_unstable();
        let mut defs = used
            .get(IdSpace::Def)
            .iter()
            .map(|&raw| {
                let def = DefId::from_raw(raw);
                Some((
                    ids.stable(IdSpace::Def, raw)?,
                    interner.is_value_def(def),
                    interner.is_shared_def(def),
                ))
            })
            .collect::<Option<Vec<_>>>()?;
        defs.sort_unstable();
        globals.sort_unstable();
        Some(hash(&(types, defs, globals)))
    }
}

fn children(ty: TypeId, interner: &TypeInterner, layouts: &LayoutTable) -> Vec<TypeId> {
    let mut out = match interner.kind(ty) {
        TyKind::Array(element) => vec![*element],
        TyKind::Struct(_, args) | TyKind::Union(_, args) | TyKind::Interface(_, args) => {
            args.clone()
        }
        TyKind::Func(params, ret) => params.iter().copied().chain([*ret]).collect(),
        TyKind::Tuple(elements) => elements.clone(),
        _ => Vec::new(),
    };
    if let Some(layout) = layouts.structs.get(&ty) {
        out.extend(layout.fields.iter().map(|field| field.ty));
    }
    if let Some(layout) = layouts.unions.get(&ty) {
        out.extend(
            layout
                .variants
                .iter()
                .flat_map(|variant| variant.fields.iter().map(|field| field.ty)),
        );
    }
    out
}

fn type_facts(
    ty: TypeId,
    interner: &TypeInterner,
    layouts: &LayoutTable,
    ids: &Ids,
) -> Option<u128> {
    let def = |def: Option<DefId>| match def {
        Some(def) => ids.stable(IdSpace::Def, def.to_raw()).map(Some),
        None => Some(None),
    };
    let fields = |fields: &[FieldLayout]| {
        fields
            .iter()
            .map(|field| {
                Some((
                    field.offset,
                    ids.ty(field.ty)?,
                    field.name.clone(),
                    field.is_weak,
                    field.is_unowned,
                ))
            })
            .collect::<Option<Vec<_>>>()
    };
    let flags = [
        interner.is_value_type(ty),
        interner.is_reference(ty),
        interner.is_rc_tracked(ty),
        interner.is_ref_struct_type(ty),
        interner.is_shared_type(ty),
        interner.is_value_union(ty),
        interner.is_niche_union(ty),
    ];
    let structure = match layouts.structs.get(&ty) {
        Some(layout) => Some((
            def(layout.destructor)?,
            &layout.name,
            fields(&layout.fields)?,
            layout.size,
            layout.align,
            layout.packed,
        )),
        None => None,
    };
    let union = match layouts.unions.get(&ty) {
        Some(layout) => Some((
            def(layout.destructor)?,
            &layout.name,
            layout
                .variants
                .iter()
                .map(|variant| Some((&variant.name, variant.discriminant, fields(&variant.fields)?)))
                .collect::<Option<Vec<_>>>()?,
            layout.size,
            layout.align,
        )),
        None => None,
    };
    Some(hash(&(flags, structure, union)))
}

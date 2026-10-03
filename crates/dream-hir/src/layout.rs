//! Target-specific memory layouts for nominal and aggregate value types.

use dream_types::{DefId, PrimTy, TyKind, TypeId, TypeInterner};
use indexmap::{IndexMap, IndexSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetLayout {
    pub ptr_size: u32,
    pub ptr_align: u32,
}

impl Default for TargetLayout {
    fn default() -> Self {
        Self {
            ptr_size: 4,
            ptr_align: 4,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LayoutFieldDef {
    pub name: String,
    pub ty: TypeId,
    pub is_weak: bool,
    pub is_unowned: bool,
}

#[derive(Debug, Clone)]
pub struct StructLayoutDef {
    pub ty: TypeId,
    pub name: String,
    pub fields: Vec<LayoutFieldDef>,
    pub packed: bool,
    pub destructor: Option<DefId>,
}

#[derive(Debug, Clone)]
pub struct UnionVariantDef {
    pub name: String,
    pub discriminant: i32,
    pub fields: Vec<LayoutFieldDef>,
}

#[derive(Debug, Clone)]
pub struct UnionLayoutDef {
    pub ty: TypeId,
    pub name: String,
    pub variants: Vec<UnionVariantDef>,
    pub destructor: Option<DefId>,
}

#[derive(Debug, Clone)]
pub struct FieldLayout {
    pub offset: u32,
    pub ty: TypeId,
    pub name: String,
    pub is_weak: bool,
    pub is_unowned: bool,
}

#[derive(Debug, Clone, Default)]
pub struct TypeLayout {
    pub destructor: Option<DefId>,
    pub name: String,
    pub fields: Vec<FieldLayout>,
    pub size: u32,
    pub align: u32,
    pub packed: bool,
}

impl TypeLayout {
    pub fn has_destructor(&self) -> bool {
        self.destructor.is_some()
    }

    /// Test/support convenience for non-nested wasm32-shaped aggregates. Production layouts are
    /// built together through [`LayoutTable::build`], which handles nested value types.
    pub fn from_fields(
        interner: &TypeInterner,
        name: impl Into<String>,
        fields: impl IntoIterator<Item = (String, TypeId, bool, bool)>,
    ) -> Self {
        direct_layout(interner, name, fields, false)
    }
}

fn direct_layout(
    interner: &TypeInterner,
    name: impl Into<String>,
    fields: impl IntoIterator<Item = (String, TypeId, bool, bool)>,
    packed: bool,
) -> TypeLayout {
    let ty = interner.void();
    let def = StructLayoutDef {
        ty,
        destructor: None,
        name: name.into(),
        fields: fields
            .into_iter()
            .map(|(name, ty, is_weak, is_unowned)| LayoutFieldDef {
                name,
                ty,
                is_weak,
                is_unowned,
            })
            .collect(),
        packed,
    };
    LayoutTable::build(TargetLayout::default(), interner, vec![def], vec![])
        .structs
        .shift_remove(&ty)
        .expect("root layout must be built")
}

#[derive(Debug, Clone)]
pub struct UnionVariant {
    pub name: String,
    pub discriminant: i32,
    pub fields: Vec<FieldLayout>,
}

#[derive(Debug, Clone, Default)]
pub struct UnionLayout {
    pub destructor: Option<DefId>,
    pub name: String,
    pub variants: Vec<UnionVariant>,
    pub size: u32,
    pub align: u32,
}

impl UnionLayout {
    pub fn has_destructor(&self) -> bool {
        self.destructor.is_some()
    }

    pub fn variant(&self, name: &str) -> Option<&UnionVariant> {
        self.variants.iter().find(|v| v.name == name)
    }
}

#[derive(Debug, Clone)]
pub struct LayoutTable {
    pub target: TargetLayout,
    pub structs: IndexMap<TypeId, TypeLayout>,
    pub unions: IndexMap<TypeId, UnionLayout>,
}

impl Default for LayoutTable {
    fn default() -> Self {
        Self::new(TargetLayout::default())
    }
}

impl LayoutTable {
    pub fn new(target: TargetLayout) -> Self {
        Self {
            target,
            structs: IndexMap::new(),
            unions: IndexMap::new(),
        }
    }

    pub fn build(
        target: TargetLayout,
        interner: &TypeInterner,
        structs: Vec<StructLayoutDef>,
        unions: Vec<UnionLayoutDef>,
    ) -> Self {
        let struct_defs: IndexMap<_, _> = structs.into_iter().map(|d| (d.ty, d)).collect();
        let union_defs: IndexMap<_, _> = unions.into_iter().map(|d| (d.ty, d)).collect();
        let mut table = Self::new(target);
        let mut active = IndexSet::new();
        for ty in struct_defs.keys().copied().collect::<Vec<_>>() {
            table.build_struct(ty, interner, &struct_defs, &union_defs, &mut active);
        }
        for ty in union_defs.keys().copied().collect::<Vec<_>>() {
            table.build_union(ty, interner, &struct_defs, &union_defs, &mut active);
        }
        table
    }

    fn build_struct(
        &mut self,
        ty: TypeId,
        interner: &TypeInterner,
        structs: &IndexMap<TypeId, StructLayoutDef>,
        unions: &IndexMap<TypeId, UnionLayoutDef>,
        active: &mut IndexSet<TypeId>,
    ) -> (u32, u32) {
        if let Some(layout) = self.structs.get(&ty) {
            return (layout.size, layout.align);
        }
        let Some(def) = structs.get(&ty) else {
            return (self.target.ptr_size, self.target.ptr_align);
        };
        assert!(
            active.insert(ty),
            "recursive value-struct layout reached HIR"
        );
        let mut offset = 0;
        // Heap objects can carry a trailing atomic lock word, which requires word alignment.
        let mut max_align = if interner.is_value_type(ty) { 1 } else { 4 };
        let mut fields = Vec::with_capacity(def.fields.len());
        for field in &def.fields {
            let (size, align) = self.size_align_inner(field.ty, interner, structs, unions, active);
            if !def.packed {
                offset = align_up(offset, align);
                max_align = max_align.max(align);
            }
            fields.push(FieldLayout {
                offset,
                ty: field.ty,
                name: field.name.clone(),
                is_weak: field.is_weak,
                is_unowned: field.is_unowned,
            });
            offset += size;
        }
        active.shift_remove(&ty);
        let align = if def.packed { 1 } else { max_align };
        let layout = TypeLayout {
            destructor: def.destructor,
            name: def.name.clone(),
            fields,
            size: if def.packed {
                offset
            } else {
                align_up(offset, align)
            },
            align,
            packed: def.packed,
        };
        let result = (layout.size, layout.align);
        self.structs.insert(ty, layout);
        result
    }

    fn build_union(
        &mut self,
        ty: TypeId,
        interner: &TypeInterner,
        structs: &IndexMap<TypeId, StructLayoutDef>,
        unions: &IndexMap<TypeId, UnionLayoutDef>,
        active: &mut IndexSet<TypeId>,
    ) -> (u32, u32) {
        if let Some(layout) = self.unions.get(&ty) {
            return (layout.size, layout.align);
        }
        let Some(def) = unions.get(&ty) else {
            return (self.target.ptr_size, self.target.ptr_align);
        };
        assert!(
            active.insert(ty),
            "recursive value-union layout reached HIR"
        );
        let mut size = 4;
        let mut max_align = 4;
        let mut variants = Vec::with_capacity(def.variants.len());
        for variant in &def.variants {
            let mut offset = 4;
            let mut fields = Vec::with_capacity(variant.fields.len());
            for field in &variant.fields {
                let (field_size, field_align) =
                    self.size_align_inner(field.ty, interner, structs, unions, active);
                offset = align_up(offset, field_align);
                fields.push(FieldLayout {
                    offset,
                    ty: field.ty,
                    name: field.name.clone(),
                    is_weak: field.is_weak,
                    is_unowned: field.is_unowned,
                });
                offset += field_size;
                max_align = max_align.max(field_align);
            }
            size = size.max(offset);
            variants.push(UnionVariant {
                name: variant.name.clone(),
                discriminant: variant.discriminant,
                fields,
            });
        }
        active.shift_remove(&ty);
        let layout = UnionLayout {
            destructor: def.destructor,
            name: def.name.clone(),
            variants,
            size: align_up(size, max_align),
            align: max_align,
        };
        let result = (layout.size, layout.align);
        self.unions.insert(ty, layout);
        result
    }

    fn size_align_inner(
        &mut self,
        ty: TypeId,
        interner: &TypeInterner,
        structs: &IndexMap<TypeId, StructLayoutDef>,
        unions: &IndexMap<TypeId, UnionLayoutDef>,
        active: &mut IndexSet<TypeId>,
    ) -> (u32, u32) {
        if interner.is_value_type(ty) {
            if structs.contains_key(&ty) {
                return self.build_struct(ty, interner, structs, unions, active);
            }
            if unions.contains_key(&ty) {
                return self.build_union(ty, interner, structs, unions, active);
            }
            panic!("value type has no layout definition: {:?}", ty);
        }
        scalar(interner, self.target, ty)
    }

    pub fn size_align(&self, interner: &TypeInterner, ty: TypeId) -> (u32, u32) {
        if interner.is_value_type(ty) {
            if let Some(layout) = self.structs.get(&ty) {
                return (layout.size, layout.align);
            }
            if let Some(layout) = self.unions.get(&ty) {
                return (layout.size, layout.align);
            }
            panic!("value type has no finalized layout: {:?}", ty);
        }
        scalar(interner, self.target, ty)
    }

    pub fn get(&self, ty: TypeId) -> Option<&TypeLayout> {
        self.structs.get(&ty)
    }
    pub fn insert(&mut self, ty: TypeId, layout: TypeLayout) {
        self.structs.insert(ty, layout);
    }
    pub fn union(&self, ty: TypeId) -> Option<&UnionLayout> {
        self.unions.get(&ty)
    }
    pub fn insert_union(&mut self, ty: TypeId, layout: UnionLayout) {
        self.unions.insert(ty, layout);
    }
}

fn scalar(interner: &TypeInterner, target: TargetLayout, ty: TypeId) -> (u32, u32) {
    match interner.kind(ty) {
        TyKind::Prim(PrimTy::String) => (target.ptr_size, target.ptr_align),
        TyKind::Prim(p) => p.size_align(),
        TyKind::Enum(_) => (4, 4),
        _ => (target.ptr_size, target.ptr_align),
    }
}

fn align_up(offset: u32, align: u32) -> u32 {
    offset.div_ceil(align) * align
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heap_payload_keeps_its_trailing_lock_word_aligned() {
        let mut interner = TypeInterner::new();
        let boolean = interner.prim(PrimTy::Bool);
        let ty = interner.struct_ty(DefId(0), vec![]);
        let table = LayoutTable::build(
            TargetLayout::default(),
            &interner,
            vec![StructLayoutDef {
                ty,
                name: "Token".into(),
                fields: vec![LayoutFieldDef {
                    name: "cancelled".into(),
                    ty: boolean,
                    is_weak: false,
                    is_unowned: false,
                }],
                packed: false,
                destructor: None,
            }],
            vec![],
        );
        let layout = table.get(ty).unwrap();
        assert_eq!((layout.size, layout.align), (4, 4));
    }

    #[test]
    fn target_pointer_width_changes_reference_fields() {
        let mut interner = TypeInterner::new();
        let string = interner.string();
        let ty = interner.tuple_ty(vec![string]);
        let def = StructLayoutDef {
            ty,
            name: "T".into(),
            fields: vec![LayoutFieldDef {
                name: "s".into(),
                ty: string,
                is_weak: false,
                is_unowned: false,
            }],
            packed: false,
            destructor: None,
        };
        let narrow = LayoutTable::build(
            TargetLayout::default(),
            &interner,
            vec![def.clone()],
            vec![],
        );
        let wide = LayoutTable::build(
            TargetLayout {
                ptr_size: 8,
                ptr_align: 8,
            },
            &interner,
            vec![def],
            vec![],
        );
        assert_eq!(
            (narrow.get(ty).unwrap().size, narrow.get(ty).unwrap().align),
            (4, 4)
        );
        assert_eq!(
            (wide.get(ty).unwrap().size, wide.get(ty).unwrap().align),
            (8, 8)
        );
    }
}

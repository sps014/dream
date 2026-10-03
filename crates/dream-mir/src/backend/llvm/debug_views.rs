//! Debugger views: DWARF composite types shaped exactly like the runtime layouts, so a local whose
//! slot holds a `dream_ptr` can be typed as a pointer to its string, array, class, union or tuple
//! block. The names (`dream_Str`, `dream_Arr_<elem>`, the class or union name) are what the lldb
//! formatters shipped by `dream debug-adapter` key on. Views only type existing slots; they add
//! no instructions.

use super::ir::MdRef;
use super::lcx::Lcx;
use super::types::is_unsigned;
use crate::backend::shared::abi_types::native_scalar_size;
use dream_types::{PrimTy, TyKind, TypeId};

/// A view member: byte offset within its composite, type, name.
type Member = (u32, TypeId, String);

fn ident(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn quoted(s: &str) -> String {
    super::ir::Metadata::string(s)[1..].to_string()
}

impl<'a> Lcx<'a> {
    /// The type a named local is shown with: a pointer to its view when its type has one, else
    /// its scalar.
    pub(super) fn di_local_ty(&mut self, ty: TypeId) -> MdRef {
        match self.di_view(ty) {
            Some(view) => self.di_ptr(view),
            None => self.di_scalar(ty),
        }
    }

    fn di_ptr(&mut self, base: MdRef) -> MdRef {
        let bits = self.cx.target.abi().ptr_size * 8;
        self.m.md.node(format!(
            "!DIDerivedType(tag: DW_TAG_pointer_type, baseType: {base}, size: {bits})"
        ))
    }

    fn di_basic(&mut self, name: &str, bits: u32, enc: &str) -> MdRef {
        self.m.md.node(format!(
            "!DIBasicType(name: {}, size: {bits}, encoding: {enc})",
            quoted(name)
        ))
    }

    /// Scalars by their Dream name; every reference without a view is the raw `dream_ptr`.
    pub(super) fn di_scalar(&mut self, ty: TypeId) -> MdRef {
        let bits = native_scalar_size(&self.cx, ty).0.max(1) * 8;
        match self.cx.interner.kind(ty) {
            TyKind::Prim(p @ (PrimTy::Float | PrimTy::Double)) => {
                self.di_basic(p.name(), bits, "DW_ATE_float")
            }
            TyKind::Prim(PrimTy::Bool) => self.di_basic("bool", bits, "DW_ATE_boolean"),
            TyKind::Prim(p) if !self.cx.interner.is_reference(ty) => {
                let enc = if is_unsigned(self.cx.interner, ty) {
                    "DW_ATE_unsigned"
                } else {
                    "DW_ATE_signed"
                };
                self.di_basic(p.name(), bits, enc)
            }
            _ if self.cx.interner.is_reference(ty) => {
                self.di_basic("dream_ptr", bits, "DW_ATE_unsigned")
            }
            _ => self.di_basic("int", bits, "DW_ATE_signed"),
        }
    }

    /// The view for `ty`, built once per name. Self-referential layouts resolve through the
    /// reserved node.
    fn di_view(&mut self, ty: TypeId) -> Option<MdRef> {
        let interner = self.cx.interner;
        let name = match interner.kind(ty) {
            TyKind::Prim(PrimTy::String) => "dream_Str".to_string(),
            TyKind::Array(elem) => format!("dream_Arr_{}", self.elem_tag(*elem)),
            TyKind::Struct(_, args) => {
                self.cx.nstruct(ty)?;
                self.alias(ty, args)
            }
            TyKind::Union(_, args) if !interner.is_niche_union(ty) => {
                self.cx.nunion(ty)?;
                self.alias(ty, args)
            }
            TyKind::Tuple(_) if interner.is_value_type(ty) => format!("tuple_{}", ty.0),
            _ => return None,
        };
        if let Some(r) = self.dbg_views.get(&name) {
            return Some(*r);
        }
        let r = self.m.md.reserve();
        self.dbg_views.insert(name.clone(), r);
        let (members, bytes) = match interner.kind(ty) {
            TyKind::Prim(_) => (self.str_members(r), 8),
            TyKind::Array(elem) => (self.arr_members(r, *elem), 4),
            TyKind::Struct(..) | TyKind::Tuple(_) => {
                let layout = self.cx.nstruct(ty)?;
                let size = layout.size;
                let fields: Vec<Member> = layout
                    .fields
                    .iter()
                    .map(|f| {
                        let name = if matches!(interner.kind(ty), TyKind::Tuple(_)) {
                            format!("t{}", f.name)
                        } else {
                            f.name.clone()
                        };
                        (f.offset, f.ty, name)
                    })
                    .collect();
                (self.members(r, &fields), size)
            }
            TyKind::Union(..) => self.union_members(r, ty)?,
            _ => return None,
        };
        let elements = self.m.md.tuple(&members);
        self.m.md.fill(
            r,
            format!(
                "!DICompositeType(tag: DW_TAG_structure_type, name: {}, size: {}, elements: {elements})",
                quoted(&name),
                bytes * 8
            ),
        );
        Some(r)
    }

    /// Generic instances keep one view per instantiation: `Box_<arg ids>`.
    fn alias(&self, ty: TypeId, args: &[TypeId]) -> String {
        let base = match self.cx.interner.kind(ty) {
            TyKind::Union(..) => self.cx.nunion(ty).map(|u| ident(&u.name)),
            _ => self.cx.nstruct(ty).map(|s| ident(&s.name)),
        }
        .unwrap_or_else(|| format!("t{}", ty.0));
        if args.is_empty() {
            return base;
        }
        let ids: Vec<String> = args.iter().map(|a| a.0.to_string()).collect();
        format!("{base}_{}", ids.join("_"))
    }

    fn elem_tag(&self, ty: TypeId) -> String {
        match self.cx.interner.kind(ty) {
            TyKind::Prim(p) => p.name().to_string(),
            TyKind::Struct(_, args) | TyKind::Union(_, args) => self.alias(ty, args),
            TyKind::Array(inner) => format!("arr_{}", self.elem_tag(*inner)),
            _ => format!("t{}", ty.0),
        }
    }

    fn member(&mut self, scope: MdRef, name: &str, base: MdRef, bits: u32, offset: u32) -> MdRef {
        self.m.md.node(format!(
            "!DIDerivedType(tag: DW_TAG_member, name: {}, scope: {scope}, baseType: {base}, \
             size: {bits}, offset: {})",
            quoted(name),
            offset * 8
        ))
    }

    fn zero_len_array(&mut self, elem: MdRef) -> MdRef {
        let range = self.m.md.node("!DISubrange(count: 0)");
        let ranges = self.m.md.tuple(&[range]);
        self.m.md.node(format!(
            "!DICompositeType(tag: DW_TAG_array_type, baseType: {elem}, size: 0, elements: {ranges})"
        ))
    }

    fn str_members(&mut self, scope: MdRef) -> Vec<MdRef> {
        let i32_ = self.di_basic("int", 32, "DW_ATE_signed");
        let u16_ = self.di_basic("unsigned short", 16, "DW_ATE_unsigned");
        let units = self.zero_len_array(u16_);
        vec![
            self.member(scope, "len", i32_, 32, 0),
            self.member(scope, "_pad", i32_, 32, 4),
            self.member(scope, "units", units, 0, 8),
        ]
    }

    /// Elements sit at offset 4, packed after the length word.
    fn arr_members(&mut self, scope: MdRef, elem: TypeId) -> Vec<MdRef> {
        let i32_ = self.di_basic("int", 32, "DW_ATE_signed");
        let e = self.di_local_ty(elem);
        let elems = self.zero_len_array(e);
        vec![
            self.member(scope, "len", i32_, 32, 0),
            self.member(scope, "elems", elems, 0, 4),
        ]
    }

    /// `(offset, type, name)` in increasing-offset order.
    fn members(&mut self, scope: MdRef, fields: &[Member]) -> Vec<MdRef> {
        fields
            .iter()
            .map(|(off, ty, name)| {
                let bits = native_scalar_size(&self.cx, *ty).0.max(1) * 8;
                let base = self.di_local_ty(*ty);
                self.member(scope, name, base, bits, *off)
            })
            .collect()
    }

    /// The discriminant word, then an anonymous union whose members are the variants in
    /// declaration order (the formatter indexes them by tag), each at its fixed block offset.
    fn union_members(&mut self, scope: MdRef, ty: TypeId) -> Option<(Vec<MdRef>, u32)> {
        let layout = self.cx.nunion(ty)?;
        let size = layout.size;
        let variants: Vec<(String, Vec<Member>)> = layout
            .variants
            .iter()
            .map(|v| {
                let fields = v
                    .fields
                    .iter()
                    .filter(|f| f.offset >= 4 && f.offset < size)
                    .map(|f| (f.offset - 4, f.ty, f.name.clone()))
                    .collect();
                (v.name.clone(), fields)
            })
            .collect();
        let payload = size.saturating_sub(4);
        let value = self.m.md.reserve();
        let mut arms = Vec::new();
        for (name, fields) in &variants {
            let st = self.m.md.reserve();
            let members = self.members(st, fields);
            let elements = self.m.md.tuple(&members);
            self.m.md.fill(
                st,
                format!(
                    "!DICompositeType(tag: DW_TAG_structure_type, size: {}, elements: {elements})",
                    payload * 8
                ),
            );
            arms.push(self.member(value, name, st, payload * 8, 0));
        }
        let elements = self.m.md.tuple(&arms);
        self.m.md.fill(
            value,
            format!(
                "!DICompositeType(tag: DW_TAG_union_type, size: {}, elements: {elements})",
                payload * 8
            ),
        );
        let i32_ = self.di_basic("int", 32, "DW_ATE_signed");
        Some((
            vec![
                self.member(scope, "tag", i32_, 32, 0),
                self.member(scope, "value", value, payload * 8, 4),
            ],
            size,
        ))
    }
}

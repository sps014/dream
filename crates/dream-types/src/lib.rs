//! The structured type system: an interner that hash-conses type shapes to compact ids, a def table
//! that names nominal declarations, and structural relations (widening/assignability) over ids.
//!
//! This replaces the historical stringly-typed representation (`Type::get_type()` producing names
//! like `Box_int`). Types are compared by [`TypeId`] equality and monomorphization is keyed by
//! `(DefId, args)` rather than by mangled strings; surface spellings are reconstructed only for
//! diagnostics via [`display::display_name`].

mod c_scalar;
mod compat;
mod def;
mod display;
mod interner;
mod kind;
mod lower;
mod naming;
mod syntax;

pub use c_scalar::CScalar;
pub use compat::{assignable, numeric_widen, overload_compatible};
pub use def::{DefInfo, DefKind, DefTable};
pub use display::{UNKNOWN_TYPE_NAME, display_name};
pub use interner::TypeInterner;
pub use kind::{PrimTy, TyKind};
pub use lower::TypeCtx;
pub use naming::{constructor_fn, json_from_json_fn, json_to_json_fn, method_fn};

/// A compact handle to an interned type. Equal ids denote structurally equal types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TypeId(pub u32);

/// A compact handle to a nominal declaration (struct/union/enum/function) in a [`DefTable`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DefId {
    pub module: ModuleId,
    pub index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct ModuleId(pub u32);

impl ModuleId {
    pub const ROOT: Self = Self(0);
}

impl DefId {
    pub const fn root(index: u32) -> Self {
        Self {
            module: ModuleId::ROOT,
            index,
        }
    }
}

impl std::fmt::Display for DefId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.module.0, self.index)
    }
}

mod symbols;
pub use symbols::{function_symbol, symbol_component, type_symbol};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interning_is_deduplicated() {
        let mut i = TypeInterner::new();
        let a = i.array(i.int());
        let b = i.array(i.int());
        assert_eq!(a, b, "identical array types must intern to the same id");
        let c = i.array(i.string());
        assert_ne!(a, c);
    }

    #[test]
    fn reference_classification() {
        let mut i = TypeInterner::new();
        assert!(!i.is_reference(i.int()));
        assert!(i.is_reference(i.string()));
        let arr = i.array(i.int());
        assert!(i.is_reference(arr));
        let fun = i.func(vec![i.int()], i.int());
        assert!(
            i.is_reference(fun),
            "fun(...) values are ARC-managed funcboxes"
        );
        let js = i.js();
        assert!(
            !i.is_reference(js),
            "js is a host handle id, not a Dream heap pointer"
        );
        assert!(
            i.is_rc_tracked(js),
            "js ownership is tracked via host retain/release"
        );
        assert!(i.is_rc_tracked(i.string()));
        assert!(!i.is_rc_tracked(i.int()));
    }

    #[test]
    fn display_renders_generics_with_angle_brackets() {
        let mut defs = DefTable::new();
        let mut i = TypeInterner::new();
        let def = defs.allocate(
            ModuleId::ROOT,
            "",
            DefKind::Struct,
            "Box",
            vec!["T".to_string()],
        );
        let boxed_int = i.struct_ty(def, vec![i.int()]);
        assert_eq!(display_name(&i, &defs, boxed_int), "Box<int>");
        let arr = i.array(i.int());
        assert_eq!(display_name(&i, &defs, arr), "int[]");
        let arr2 = i.array(arr);
        assert_eq!(display_name(&i, &defs, arr2), "int[][]");
        let list = defs.allocate(
            ModuleId::ROOT,
            "",
            DefKind::Struct,
            "List",
            vec!["T".to_string()],
        );
        let list_int = i.struct_ty(list, vec![i.int()]);
        assert_eq!(display_name(&i, &defs, list_int), "List<int>");
        let nested = i.struct_ty(list, vec![list_int]);
        assert_eq!(display_name(&i, &defs, nested), "List<List<int>>");
        let list_arr = i.struct_ty(list, vec![arr]);
        assert_eq!(display_name(&i, &defs, list_arr), "List<int[]>");
    }

    #[test]
    fn definitions_have_module_local_identity() {
        let mut defs = DefTable::new();
        let a = defs.allocate(ModuleId(1), "a", DefKind::Struct, "User", vec![]);
        let b = defs.allocate(ModuleId(2), "b", DefKind::Struct, "User", vec![]);
        assert_ne!(a, b);
        assert_eq!(a.index, b.index);
        assert_eq!(defs.name(a), defs.name(b));
        let f = defs.allocate(ModuleId(1), "a", DefKind::Function, "User", vec![]);
        assert_eq!(f.index, a.index + 1);
    }

    #[test]
    fn assignability_rules() {
        let mut defs = DefTable::new();
        let mut i = TypeInterner::new();

        // Identity and object-top.
        assert!(assignable(&i, i.int(), i.int()));
        assert!(assignable(&i, i.object(), i.string()));

        // Numeric widening is directional.
        let long = i.prim(PrimTy::Long);
        assert!(assignable(&i, long, i.int()));
        assert!(!assignable(&i, i.int(), long));

        // Enum <-> int.
        let color = defs.allocate(ModuleId::ROOT, "", DefKind::Enum, "Color", vec![]);
        let color_ty = i.enum_ty(color);
        assert!(assignable(&i, color_ty, i.int()));
        assert!(assignable(&i, i.int(), color_ty));

        // Poison is bidirectionally compatible.
        assert!(assignable(&i, i.int(), i.error()));
        assert!(assignable(&i, i.error(), i.string()));
    }

    #[test]
    fn overload_compatibility_is_loose_on_numerics() {
        let mut i = TypeInterner::new();
        // Narrowing direction is fine for overload viability though not for assignment.
        let long = i.prim(PrimTy::Long);
        assert!(overload_compatible(&i, i.int(), long));
        assert!(overload_compatible(&i, long, i.int()));
        assert!(!overload_compatible(&i, i.int(), i.string()));
    }
}

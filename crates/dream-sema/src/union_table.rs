//! Resolved variants and payload types for discriminated unions. Target-specific storage layout
//! belongs to the HIR layout table.

use dream_syntax::nodes::Type;
use indexmap::IndexMap;

/// A single payload field of a union variant.
#[derive(Debug, Clone)]
pub struct UnionFieldInfo {
    pub name: String,
    pub type_: Type,
}

/// A single variant of a discriminated union.
#[derive(Debug, Clone)]
pub struct UnionVariantInfo {
    pub name: String,
    /// The discriminant stored at offset 0 to identify this variant at runtime.
    pub discriminant: i32,
    pub fields: Vec<UnionFieldInfo>,
}

/// The resolved variants of a (monomorphized) discriminated union.
#[derive(Debug, Clone)]
pub struct UnionInfo {
    pub name: String,
    pub variants: Vec<UnionVariantInfo>,
}

impl UnionInfo {
    pub fn variant(&self, name: &str) -> Option<&UnionVariantInfo> {
        self.variants.iter().find(|v| v.name == name)
    }
}

/// Registered (monomorphized) unions: name -> variants. Insertion-ordered so the union protocol
/// defaults and release code emit in a deterministic (registration) order.
pub type UnionTable = IndexMap<String, UnionInfo>;

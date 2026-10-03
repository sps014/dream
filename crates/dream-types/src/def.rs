//! Module-scoped declaration storage. Source-name resolution belongs to the frontend resolver.

use super::{DefId, ModuleId};
use indexmap::IndexMap;

/// What a [`DefId`] names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DefKind {
    Struct,
    Union,
    Enum,
    Function,
    Interface,
}

/// A single nominal declaration. `generic_params` records the declared type-parameter names (e.g.
/// `["T", "V"]`) in order; it is empty for non-generic defs. `name` is the source-level base name
/// (never a mangled monomorphization name).
#[derive(Debug, Clone)]
pub struct DefInfo {
    pub module_path: String,
    pub kind: DefKind,
    pub name: String,
    pub generic_params: Vec<String>,
    /// True for `struct` (value) declarations: instances are stored inline with copy semantics
    /// rather than as heap-allocated, reference-counted objects. Always `false` for non-structs.
    pub is_value: bool,
    /// True for `static class` declarations: a namespace of static members, not a value type.
    pub is_static: bool,
}

/// Definitions are indexed only by identity; equal source names in distinct modules are unrelated.
#[derive(Debug, Default)]
pub struct DefTable {
    defs: IndexMap<DefId, DefInfo>,
    next_index: IndexMap<ModuleId, u32>,
}

impl DefTable {
    pub fn new() -> Self {
        DefTable::default()
    }

    pub fn allocate(
        &mut self,
        module: ModuleId,
        module_path: &str,
        kind: DefKind,
        name: &str,
        generic_params: Vec<String>,
    ) -> DefId {
        let next = self.next_index.entry(module).or_default();
        let id = DefId {
            module,
            index: *next,
        };
        *next += 1;
        self.defs.insert(
            id,
            DefInfo {
                module_path: module_path.to_string(),
                kind,
                name: name.to_string(),
                generic_params,
                is_value: false,
                is_static: false,
            },
        );
        id
    }

    /// Marks a definition as a value (`struct`) type. Idempotent.
    pub fn mark_value(&mut self, id: DefId) {
        self.defs
            .get_mut(&id)
            .expect("registered definition")
            .is_value = true;
    }

    /// True when `id` names a value (`struct`) type.
    pub fn is_value(&self, id: DefId) -> bool {
        self.defs[&id].is_value
    }

    /// Marks a definition as a `static class`. Idempotent.
    pub fn mark_static(&mut self, id: DefId) {
        self.defs
            .get_mut(&id)
            .expect("registered definition")
            .is_static = true;
    }

    /// True when `id` names a `static class`.
    pub fn is_static(&self, id: DefId) -> bool {
        self.defs[&id].is_static
    }

    pub fn get(&self, id: DefId) -> &DefInfo {
        &self.defs[&id]
    }

    pub fn set_generic_params(&mut self, id: DefId, params: Vec<String>) {
        self.defs
            .get_mut(&id)
            .expect("registered definition")
            .generic_params = params;
    }

    pub fn name(&self, id: DefId) -> &str {
        &self.defs[&id].name
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }
}

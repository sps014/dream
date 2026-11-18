//! Names for the types and declarations a persisted body refers to that survive recompilation.
//!
//! A type is named by its shape. A declaration is named by its module, kind and source name; a
//! function adds its owner and resolved signature, and same-named declarations that still
//! coincide are told apart by allocation order within their module, which only content the
//! cache key covers can change.

use crate::analyzer::Analyzer;
use dream_types::{DefId, DefKind, PrimTy, TyKind, TypeId};
use indexmap::IndexMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TypeKey {
    Prim(PrimTy),
    Object,
    Void,
    Error,
    Js,
    Array(Box<TypeKey>),
    Struct(DefKey, Vec<TypeKey>),
    Union(DefKey, Vec<TypeKey>),
    Interface(DefKey, Vec<TypeKey>),
    Enum(DefKey),
    Func(Vec<TypeKey>, Box<TypeKey>),
    Tuple(Vec<TypeKey>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct DefKey {
    module: String,
    kind: DefKind,
    name: String,
    signature: Option<Signature>,
    ordinal: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
struct Signature {
    owner: Option<Box<TypeKey>>,
    shape: Option<(Vec<TypeKey>, Box<TypeKey>)>,
}

/// Same-named declarations per module in allocation order, and the owner type of each method.
#[derive(Default)]
pub(in crate::analyzer) struct DefIndex {
    indexed_defs: usize,
    named: IndexMap<(String, DefKind, String), Vec<DefId>>,
    indexed_functions: usize,
    owners: IndexMap<DefId, TypeId>,
}

impl Analyzer<'_> {
    pub(super) fn refresh_def_index(&mut self) {
        let index = &mut self.def_index;
        for (id, info) in self.type_ctx.defs.iter_from(index.indexed_defs) {
            index
                .named
                .entry((info.module_path.clone(), info.kind, info.name.clone()))
                .or_default()
                .push(id);
        }
        index.indexed_defs = self.type_ctx.defs.len();
        let table = &self.function_table;
        let functions = table.functions.len() + table.generic_method_overloads.len();
        if functions == index.indexed_functions {
            return;
        }
        index.indexed_functions = functions;
        index.owners.clear();
        for ((owner, _), identities) in &table.methods {
            for (def, _) in identities {
                index.owners.entry(*def).or_insert(*owner);
            }
        }
        for ((owner, _), defs) in &table.generic_method_overloads {
            for def in defs {
                index.owners.entry(*def).or_insert(*owner);
            }
        }
    }

    pub(super) fn type_key(&self, ty: TypeId) -> TypeKey {
        let keys = |ids: &[TypeId]| ids.iter().map(|&id| self.type_key(id)).collect();
        match self.type_ctx.interner.kind(ty) {
            TyKind::Prim(prim) => TypeKey::Prim(*prim),
            TyKind::Object => TypeKey::Object,
            TyKind::Void => TypeKey::Void,
            TyKind::Error => TypeKey::Error,
            TyKind::Js => TypeKey::Js,
            TyKind::Array(element) => TypeKey::Array(Box::new(self.type_key(*element))),
            TyKind::Struct(def, args) => TypeKey::Struct(self.def_key(*def), keys(args)),
            TyKind::Union(def, args) => TypeKey::Union(self.def_key(*def), keys(args)),
            TyKind::Interface(def, args) => TypeKey::Interface(self.def_key(*def), keys(args)),
            TyKind::Enum(def) => TypeKey::Enum(self.def_key(*def)),
            TyKind::Func(params, ret) => TypeKey::Func(keys(params), Box::new(self.type_key(*ret))),
            TyKind::Tuple(elements) => TypeKey::Tuple(keys(elements)),
        }
    }

    pub(super) fn resolve_type(&self, key: &TypeKey) -> Option<TypeId> {
        self.type_ctx.interner.lookup(&self.resolve_kind(key)?)
    }

    /// Interns the type `key` names; its components must already exist.
    pub(super) fn intern_type_key(&mut self, key: &TypeKey) -> Option<TypeId> {
        let kind = self.resolve_kind(key)?;
        Some(self.type_ctx.interner.intern(kind))
    }

    fn resolve_kind(&self, key: &TypeKey) -> Option<TyKind> {
        let ids = |keys: &[TypeKey]| -> Option<Vec<TypeId>> {
            keys.iter().map(|key| self.resolve_type(key)).collect()
        };
        Some(match key {
            TypeKey::Prim(prim) => TyKind::Prim(*prim),
            TypeKey::Object => TyKind::Object,
            TypeKey::Void => TyKind::Void,
            TypeKey::Error => TyKind::Error,
            TypeKey::Js => TyKind::Js,
            TypeKey::Array(element) => TyKind::Array(self.resolve_type(element)?),
            TypeKey::Struct(def, args) => TyKind::Struct(self.resolve_def(def)?, ids(args)?),
            TypeKey::Union(def, args) => TyKind::Union(self.resolve_def(def)?, ids(args)?),
            TypeKey::Interface(def, args) => TyKind::Interface(self.resolve_def(def)?, ids(args)?),
            TypeKey::Enum(def) => TyKind::Enum(self.resolve_def(def)?),
            TypeKey::Func(params, ret) => TyKind::Func(ids(params)?, self.resolve_type(ret)?),
            TypeKey::Tuple(elements) => TyKind::Tuple(ids(elements)?),
        })
    }

    pub(super) fn def_key(&self, def: DefId) -> DefKey {
        let info = self.type_ctx.defs.get(def);
        let signature = self.def_signature(def, info.kind);
        let ordinal = self
            .same_named(&info.module_path, info.kind, &info.name)
            .iter()
            .take_while(|&&id| id != def)
            .filter(|&&id| self.def_signature(id, info.kind) == signature)
            .count();
        DefKey {
            module: info.module_path.clone(),
            kind: info.kind,
            name: info.name.clone(),
            signature,
            ordinal: ordinal as u32,
        }
    }

    pub(super) fn resolve_def(&self, key: &DefKey) -> Option<DefId> {
        self.same_named(&key.module, key.kind, &key.name)
            .iter()
            .copied()
            .filter(|&id| self.def_signature(id, key.kind) == key.signature)
            .nth(key.ordinal as usize)
    }

    fn same_named(&self, module: &str, kind: DefKind, name: &str) -> &[DefId] {
        self.def_index
            .named
            .get(&(module.to_string(), kind, name.to_string()))
            .map_or(&[], Vec::as_slice)
    }

    fn def_signature(&self, def: DefId, kind: DefKind) -> Option<Signature> {
        if kind != DefKind::Function {
            return None;
        }
        let shape = self
            .function_table
            .functions
            .get(&(def, Vec::new()))
            .map(|info| {
                (
                    info.parameters.iter().map(|&ty| self.type_key(ty)).collect(),
                    Box::new(self.type_key(info.resolved_return)),
                )
            });
        Some(Signature {
            owner: self
                .def_index
                .owners
                .get(&def)
                .map(|&ty| Box::new(self.type_key(ty))),
            shape,
        })
    }
}

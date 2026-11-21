//! Stable identities for the ids a persisted function refers to.
//!
//! A type is named by its shape. A declaration is named by its module, kind and name, plus its
//! position among same-named declarations of that module in allocation order. A global is named
//! by its program-unique source name and an interface by its type. Identities that collide
//! within one compilation are dropped, so every id that has one names exactly one thing.

use crate::Mir;
use dream_types::portable::{IdSpace, StableIds};
use dream_types::{DefId, DefTable, TyKind, TypeId, TypeInterner};
use indexmap::IndexMap;

/// Stable identities of every declaration, computed once per compilation.
#[derive(Debug, Default)]
pub(super) struct DefIds {
    pub(super) stable: IndexMap<u64, u128>,
    /// Each declaration's module path, which selects the cache file its functions live in.
    pub(super) modules: IndexMap<u64, String>,
}

impl DefIds {
    pub(super) fn new(defs: &DefTable) -> Self {
        let mut ordinals: IndexMap<(&str, dream_types::DefKind, &str), u32> = IndexMap::new();
        let mut ids = Self::default();
        for (def, info) in defs.iter_from(0) {
            let ordinal = ordinals
                .entry((&info.module_path, info.kind, &info.name))
                .or_default();
            let identity = hash(&(
                "def",
                &info.module_path,
                format!("{:?}", info.kind),
                &info.name,
                *ordinal,
            ));
            *ordinal += 1;
            ids.stable.insert(def.to_raw(), identity);
            ids.modules.insert(def.to_raw(), info.module_path.clone());
        }
        ids
    }

    pub(super) fn module(&self, def: DefId) -> Option<&str> {
        self.modules.get(&def.to_raw()).map(String::as_str)
    }
}

/// The identities of one compilation's ids, in both directions.
pub(super) struct Ids {
    spaces: [Space; 4],
}

#[derive(Default)]
struct Space {
    stable: IndexMap<u64, u128>,
    local: IndexMap<u128, u64>,
}

impl Space {
    fn build(pairs: impl IntoIterator<Item = (u64, u128)>) -> Self {
        let mut stable = IndexMap::new();
        let mut local: IndexMap<u128, Option<u64>> = IndexMap::new();
        for (raw, identity) in pairs {
            stable.insert(raw, identity);
            local
                .entry(identity)
                .and_modify(|slot| *slot = None)
                .or_insert(Some(raw));
        }
        let local: IndexMap<u128, u64> = local
            .into_iter()
            .filter_map(|(identity, raw)| Some((identity, raw?)))
            .collect();
        stable.retain(|_, identity| local.contains_key(identity));
        Self { stable, local }
    }
}

impl Ids {
    pub(super) fn new(mir: &Mir, interner: &TypeInterner, defs: &DefIds) -> Self {
        let mut types: Vec<Option<u128>> = Vec::with_capacity(interner.len());
        for (_, kind) in interner.iter_kinds() {
            types.push(type_identity(kind, &types, defs));
        }
        let interfaces = mir
            .interfaces
            .interfaces
            .iter()
            .enumerate()
            .filter_map(|(index, info)| {
                let ty = types.get(info.ty.0 as usize).copied().flatten()?;
                Some((index as u64, hash(&("interface", ty))))
            });
        let globals = mir
            .globals
            .iter()
            .map(|global| (global.id.0 as u64, hash(&("global", &global.name))));
        Self {
            spaces: [
                Space::build(
                    types
                        .iter()
                        .enumerate()
                        .filter_map(|(raw, identity)| Some((raw as u64, (*identity)?))),
                ),
                Space::build(defs.stable.iter().map(|(&raw, &identity)| (raw, identity))),
                Space::build(globals),
                Space::build(interfaces),
            ],
        }
    }

    pub(super) fn ty(&self, ty: TypeId) -> Option<u128> {
        self.stable(IdSpace::Type, ty.0 as u64)
    }
}

impl StableIds for Ids {
    fn stable(&self, space: IdSpace, raw: u64) -> Option<u128> {
        self.spaces[space as usize].stable.get(&raw).copied()
    }

    fn local(&self, space: IdSpace, stable: u128) -> Option<u64> {
        self.spaces[space as usize].local.get(&stable).copied()
    }
}

/// Components are interned before the types built from them, so their identities exist.
fn type_identity(kind: &TyKind, types: &[Option<u128>], defs: &DefIds) -> Option<u128> {
    let ty = |id: &TypeId| types.get(id.0 as usize).copied().flatten();
    let list = |ids: &[TypeId]| ids.iter().map(ty).collect::<Option<Vec<_>>>();
    let def = |id: &DefId| defs.stable.get(&id.to_raw()).copied();
    Some(match kind {
        TyKind::Prim(prim) => hash(&("prim", format!("{prim:?}"))),
        TyKind::Object => hash(&"object"),
        TyKind::Void => hash(&"void"),
        TyKind::Error => hash(&"error"),
        TyKind::Js => hash(&"js"),
        TyKind::Array(element) => hash(&("array", ty(element)?)),
        TyKind::Struct(id, args) => hash(&("struct", def(id)?, list(args)?)),
        TyKind::Union(id, args) => hash(&("union", def(id)?, list(args)?)),
        TyKind::Interface(id, args) => hash(&("interface", def(id)?, list(args)?)),
        TyKind::Enum(id) => hash(&("enum", def(id)?)),
        TyKind::Func(params, ret) => hash(&("func", list(params)?, ty(ret)?)),
        TyKind::Tuple(elements) => hash(&("tuple", list(elements)?)),
    })
}

pub(super) fn hash(value: &impl serde::Serialize) -> u128 {
    let bytes = postcard::to_allocvec(value).unwrap_or_default();
    let digest = blake3::hash(&bytes);
    let mut low = [0; 16];
    low.copy_from_slice(&digest.as_bytes()[..16]);
    u128::from_le_bytes(low)
}

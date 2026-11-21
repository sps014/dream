//! Serialization of compilation-local ids for persisted typed bodies.
//!
//! `TypeId`, `DefId` and the HIR's global and interface indices depend on the order in which one
//! compilation registered and interned everything. A persisted body therefore never stores them
//! directly: [`encode`] replaces every id with an index into a per-space table that the caller
//! converts to stable keys, and [`decode`] maps those indices back to the ids of the current
//! compilation. [`encode_stable`] and [`decode_stable`] instead write each id as a stable
//! identity that names it in every compilation, through a [`StableIds`] map. Outside every scope,
//! ids serialize as their raw values.

use std::cell::RefCell;
use std::sync::Arc;

/// The kinds of compilation-local ids, each with its own translation table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdSpace {
    Type,
    Def,
    Global,
    Interface,
}

const SPACES: usize = 4;

/// Raw ids of each space in first-use order; the serialized form refers to them by position.
#[derive(Debug, Default, Clone)]
pub struct IdTables(pub [Vec<u64>; SPACES]);

impl IdTables {
    pub fn get(&self, space: IdSpace) -> &[u64] {
        &self.0[space as usize]
    }
}

/// Identities of compilation-local ids that are equal across compilations exactly when the ids
/// name the same thing. Within one compilation the mapping must be one to one.
pub trait StableIds: Send + Sync {
    fn stable(&self, space: IdSpace, raw: u64) -> Option<u128>;
    fn local(&self, space: IdSpace, stable: u128) -> Option<u64>;
}

enum Scope {
    Encode([indexmap::IndexSet<u64>; SPACES]),
    Decode(IdTables),
    /// The raw ids written so far, in first-use order.
    Stable(Arc<dyn StableIds>, [indexmap::IndexSet<u64>; SPACES]),
}

thread_local! {
    static SCOPE: RefCell<Option<Scope>> = const { RefCell::new(None) };
}

struct Reset(Option<Scope>);

impl Drop for Reset {
    fn drop(&mut self) {
        let previous = self.0.take();
        SCOPE.with(|scope| *scope.borrow_mut() = previous);
    }
}

fn enter(scope: Scope) -> Reset {
    Reset(SCOPE.with(|current| current.borrow_mut().replace(scope)))
}

/// Runs `serialize` with ids replaced by table positions and returns the tables it used.
pub fn encode<T>(serialize: impl FnOnce() -> T) -> (T, IdTables) {
    let reset = enter(Scope::Encode(Default::default()));
    let value = serialize();
    let tables = SCOPE.with(|scope| match scope.borrow_mut().take() {
        Some(Scope::Encode(sets)) => IdTables(sets.map(|set| set.into_iter().collect())),
        _ => IdTables::default(),
    });
    drop(reset);
    (value, tables)
}

/// Runs `deserialize` with table positions resolved through `tables`.
pub fn decode<T>(tables: IdTables, deserialize: impl FnOnce() -> T) -> T {
    let _reset = enter(Scope::Decode(tables));
    deserialize()
}

/// Runs `serialize` with every id written as its stable identity and returns the raw ids it
/// wrote. Serializing an id that `ids` cannot name fails.
pub fn encode_stable<T>(ids: Arc<dyn StableIds>, serialize: impl FnOnce() -> T) -> (T, IdTables) {
    let reset = enter(Scope::Stable(ids, Default::default()));
    let value = serialize();
    let used = SCOPE.with(|scope| match scope.borrow_mut().take() {
        Some(Scope::Stable(_, sets)) => IdTables(sets.map(|set| set.into_iter().collect())),
        _ => IdTables::default(),
    });
    drop(reset);
    (value, used)
}

/// Runs `deserialize` with stable identities resolved to the ids of this compilation.
pub fn decode_stable<T>(ids: Arc<dyn StableIds>, deserialize: impl FnOnce() -> T) -> T {
    let _reset = enter(Scope::Stable(ids, Default::default()));
    deserialize()
}

enum Written {
    Raw(u64),
    Stable(Option<u128>),
}

pub fn serialize_id<S: serde::Serializer>(
    space: IdSpace,
    raw: u64,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let written = SCOPE.with(|scope| match &mut *scope.borrow_mut() {
        Some(Scope::Encode(sets)) => Written::Raw(sets[space as usize].insert_full(raw).0 as u64),
        Some(Scope::Stable(ids, used)) => {
            used[space as usize].insert(raw);
            Written::Stable(ids.stable(space, raw))
        }
        _ => Written::Raw(raw),
    });
    match written {
        Written::Raw(value) => serializer.serialize_u64(value),
        Written::Stable(Some(stable)) => serializer.serialize_u128(stable),
        Written::Stable(None) => Err(serde::ser::Error::custom("id without a stable identity")),
    }
}

pub fn deserialize_id<'de, D: serde::Deserializer<'de>>(
    space: IdSpace,
    deserializer: D,
) -> Result<u64, D::Error> {
    let ids = SCOPE.with(|scope| match &*scope.borrow() {
        Some(Scope::Stable(ids, _)) => Some(ids.clone()),
        _ => None,
    });
    if let Some(ids) = ids {
        let stable = <u128 as serde::Deserialize>::deserialize(deserializer)?;
        return ids
            .local(space, stable)
            .ok_or_else(|| serde::de::Error::custom("unknown stable id"));
    }
    let value = <u64 as serde::Deserialize>::deserialize(deserializer)?;
    SCOPE.with(|scope| match &*scope.borrow() {
        Some(Scope::Decode(tables)) => tables
            .get(space)
            .get(value as usize)
            .copied()
            .ok_or_else(|| serde::de::Error::custom("unknown persisted id")),
        _ => Ok(value),
    })
}

impl serde::Serialize for crate::TypeId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serialize_id(IdSpace::Type, self.0 as u64, serializer)
    }
}

impl<'de> serde::Deserialize<'de> for crate::TypeId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = deserialize_id(IdSpace::Type, deserializer)?;
        u32::try_from(raw)
            .map(crate::TypeId)
            .map_err(serde::de::Error::custom)
    }
}

impl crate::DefId {
    pub fn to_raw(self) -> u64 {
        ((self.module.0 as u64) << 32) | self.index as u64
    }

    pub fn from_raw(raw: u64) -> Self {
        Self {
            module: crate::ModuleId((raw >> 32) as u32),
            index: raw as u32,
        }
    }
}

impl serde::Serialize for crate::DefId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serialize_id(IdSpace::Def, self.to_raw(), serializer)
    }
}

impl<'de> serde::Deserialize<'de> for crate::DefId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserialize_id(IdSpace::Def, deserializer).map(Self::from_raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DefId, ModuleId, TypeId};

    #[test]
    fn encoded_ids_resolve_through_replacement_tables() {
        let def = DefId {
            module: ModuleId(3),
            index: 9,
        };
        let (json, tables) =
            encode(|| serde_json::to_string(&(TypeId(40), def, TypeId(7), TypeId(40))).unwrap());
        assert_eq!(json, "[0,0,1,0]");
        assert_eq!(tables.get(IdSpace::Type), &[40, 7]);
        assert_eq!(tables.get(IdSpace::Def), &[def.to_raw()]);

        let mut current = IdTables::default();
        current.0[IdSpace::Type as usize] = vec![5, 6];
        current.0[IdSpace::Def as usize] = vec![DefId::root(2).to_raw()];
        let decoded: (TypeId, DefId, TypeId, TypeId) =
            decode(current, || serde_json::from_str(&json).unwrap());
        assert_eq!(decoded, (TypeId(5), DefId::root(2), TypeId(6), TypeId(5)));
        assert_eq!(serde_json::to_string(&TypeId(40)).unwrap(), "40");
    }

    /// A compilation that numbers every type the given offset higher. Type 13 has no identity.
    struct Shifted(u64);

    impl StableIds for Shifted {
        fn stable(&self, space: IdSpace, raw: u64) -> Option<u128> {
            (space == IdSpace::Type && raw != 13).then(|| (raw - self.0) as u128)
        }

        fn local(&self, space: IdSpace, stable: u128) -> Option<u64> {
            (space == IdSpace::Type).then(|| stable as u64 + self.0)
        }
    }

    #[test]
    fn stable_identities_carry_ids_between_numberings() {
        let (json, used) = encode_stable(Arc::new(Shifted(0)), || {
            serde_json::to_string(&(TypeId(7), TypeId(9), TypeId(7))).unwrap()
        });
        assert_eq!(used.get(IdSpace::Type), &[7, 9]);
        let decoded: (TypeId, TypeId, TypeId) =
            decode_stable(Arc::new(Shifted(1000)), || serde_json::from_str(&json).unwrap());
        assert_eq!(decoded, (TypeId(1007), TypeId(1009), TypeId(1007)));
        let (unnamed, _) =
            encode_stable(Arc::new(Shifted(0)), || serde_json::to_string(&TypeId(13)));
        assert!(unnamed.is_err());
    }
}

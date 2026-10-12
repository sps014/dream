use dream_types::{DefId, TypeId};
use indexmap::Equivalent;

/// Borrowing the instance arguments keeps hot summary lookups from cloning their key vector.
#[derive(Hash)]
pub(crate) struct FunctionKeyRef<'a> {
    pub def: DefId,
    pub args: &'a [TypeId],
}

impl Equivalent<(DefId, Vec<TypeId>)> for FunctionKeyRef<'_> {
    fn equivalent(&self, key: &(DefId, Vec<TypeId>)) -> bool {
        self.def == key.0 && self.args == key.1.as_slice()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::hash::{DefaultHasher, Hash, Hasher};

    #[test]
    fn borrowed_instance_keys_hash_like_owned_keys_and_find_only_exact_instances() {
        let ctx = dream_types::TypeCtx::new();
        let def = DefId::root(9);
        let args = vec![ctx.interner.int(), ctx.interner.long()];
        let owned = (def, args.clone());
        let borrowed = FunctionKeyRef { def, args: &args };
        let mut owned_hash = DefaultHasher::new();
        owned.hash(&mut owned_hash);
        let mut borrowed_hash = DefaultHasher::new();
        borrowed.hash(&mut borrowed_hash);
        assert_eq!(owned_hash.finish(), borrowed_hash.finish());
        let mut map = indexmap::IndexMap::new();
        map.insert(owned, 42);
        assert_eq!(map.get(&borrowed), Some(&42));
        assert_eq!(
            map.get(&FunctionKeyRef {
                def,
                args: &args[..1]
            }),
            None
        );
        assert_eq!(
            map.get(&FunctionKeyRef {
                def: DefId::root(10),
                args: &args
            }),
            None
        );
    }
}

//! Which codegen unit each function is emitted into. Every source module gets its own units so an
//! edit recompiles only the module it touched; unit 0 keeps the glue and functions without a
//! source file. A module with many functions is split into buckets by a hash of each symbol, so
//! an edit invalidates one bucket and adding a function rarely moves the others.

use crate::Mir;
use std::collections::BTreeMap;

/// The function count a bucket is sized for.
const BUCKET_FUNCTIONS: usize = 256;
/// At most `2^MAX_BUCKET_BITS` buckets per module.
const MAX_BUCKET_BITS: u32 = 6;

/// The unit of each function in `mir.functions`, in order. `files` maps a source file to its
/// module's group, numbered from 1.
pub(super) fn assign(mir: &Mir, files: &BTreeMap<&str, usize>) -> Vec<usize> {
    let groups: Vec<usize> = mir
        .functions
        .iter()
        .map(|f| f.file.as_deref().and_then(|path| files.get(path)).copied().unwrap_or(0))
        .collect();
    let mut counts = BTreeMap::<usize, usize>::new();
    for &group in &groups {
        *counts.entry(group).or_default() += 1;
    }
    mir.functions
        .iter()
        .zip(groups)
        .map(|(f, group)| {
            if group == 0 {
                return 0;
            }
            let bits = bucket_bits(counts[&group]);
            let bucket = symbol_hash(&f.symbol) & ((1 << bits) - 1);
            (group << MAX_BUCKET_BITS) | bucket
        })
        .collect()
}

fn bucket_bits(functions: usize) -> u32 {
    let mut bits = 0;
    while bits < MAX_BUCKET_BITS && functions > BUCKET_FUNCTIONS << bits {
        bits += 1;
    }
    bits
}

fn symbol_hash(symbol: &str) -> usize {
    let digest = blake3::hash(symbol.as_bytes());
    let bytes: [u8; 8] = digest.as_bytes()[..8].try_into().expect("blake3 digest is 32 bytes");
    u64::from_le_bytes(bytes) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modules_split_only_past_the_bucket_size() {
        assert_eq!(bucket_bits(1), 0);
        assert_eq!(bucket_bits(BUCKET_FUNCTIONS), 0);
        assert_eq!(bucket_bits(BUCKET_FUNCTIONS + 1), 1);
        assert_eq!(bucket_bits(BUCKET_FUNCTIONS * 4), 2);
        assert_eq!(bucket_bits(usize::MAX / 2), MAX_BUCKET_BITS);
    }

    #[test]
    fn a_symbol_keeps_its_bucket() {
        assert_eq!(symbol_hash("dream_main"), symbol_hash("dream_main"));
        assert_ne!(symbol_hash("dream_main"), symbol_hash("dream_main2"));
    }
}

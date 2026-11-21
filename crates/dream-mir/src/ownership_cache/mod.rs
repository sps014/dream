//! Persisted ownership preparation results: each function's local mod/ref summary and its
//! reference-count-inserted body.
//!
//! Ids are stored as stable identities ([`ids`]). A function's body digest covers its prepared
//! body and the layouts and value-ness of every type and global it names ([`facts`]), which is
//! everything either computation reads from the function itself.
//!
//! - A summary is reused when the body digest, the function's cleanup bit and the module-wide
//!   summary switches match.
//! - An inserted body additionally records the mod/ref queries its insertion made with a digest
//!   of each answer, and is reused only while every query still gives the same answer and the
//!   table-wide switches match. Insertion reads nothing else, so fresh insertion would produce
//!   the same body.

mod facts;
mod ids;

use crate::analysis::effects::log::{self, Query};
use crate::analysis::effects::ModRefTable;
use crate::{Mir, MirFunction};
use dream_hir::LayoutTable;
use dream_types::portable::{self, IdSpace, StableIds};
use dream_types::{DefId, DefTable, TypeInterner};
use indexmap::{IndexMap, IndexSet};
use std::sync::Arc;

const FORMAT: u32 = 2;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Entry {
    /// The summary's key and its stable encoding.
    summary: Option<(u128, Vec<u8>)>,
    /// The inserted body's key and the stable encoding of its queries and body.
    inserted: Option<(u128, Vec<u8>)>,
}

type Entries = IndexMap<String, Entry>;

/// One project's persisted results, grouped by the source module of each function.
#[derive(Default)]
pub struct OwnershipCache {
    defs: ids::DefIds,
    previous: IndexMap<String, Entries>,
    current: IndexMap<String, Entries>,
    reused: usize,
}

impl OwnershipCache {
    pub fn new(defs: &DefTable) -> Self {
        Self {
            defs: ids::DefIds::new(defs),
            ..Self::default()
        }
    }

    /// The modules whose cache files `mir`'s functions may use.
    pub fn modules(&self, mir: &Mir) -> IndexSet<String> {
        mir.functions
            .iter()
            .chain(&mir.polls)
            .filter_map(|f| self.defs.module(f.def))
            .map(str::to_string)
            .collect()
    }

    /// Adds a module's entries from a cache file; unreadable contents are ignored.
    pub fn load(&mut self, module: &str, bytes: &[u8]) {
        if let Ok((FORMAT, entries)) = postcard::from_bytes::<(u32, Vec<(String, Entry)>)>(bytes) {
            self.previous
                .insert(module.to_string(), entries.into_iter().collect());
        }
    }

    /// How many functions took a cached inserted body in the last preparation.
    pub fn reused(&self) -> usize {
        self.reused
    }

    /// Cache file contents for each module whose entries changed.
    pub fn changed(&self) -> Vec<(&str, Vec<u8>)> {
        self.current
            .iter()
            .filter(|(module, entries)| self.previous.get(*module) != Some(entries))
            .filter_map(|(module, entries)| {
                let entries: Vec<(&String, &Entry)> = entries.iter().collect();
                Some((
                    module.as_str(),
                    postcard::to_allocvec(&(FORMAT, entries)).ok()?,
                ))
            })
            .collect()
    }

    /// Digests every prepared function; `None` when the program's ids cannot all be named.
    pub(crate) fn begin(&mut self, mir: &Mir, interner: &TypeInterner) -> Option<Session> {
        use rayon::prelude::*;
        self.current.clear();
        self.reused = 0;
        let ids = Arc::new(ids::Ids::new(mir, interner, &self.defs));
        let facts = facts::Facts::new(mir, interner, &ids);
        let keys = self.keys(mir);
        let bodies = mir
            .functions
            .par_iter()
            .chain(mir.polls.par_iter())
            .zip(keys.par_iter())
            .map(|(f, key)| {
                key.as_ref()?;
                body_digest(f, interner, &mir.layouts, &ids, &facts)
            })
            .collect();
        let mut counters = mir
            .intrinsics
            .iter()
            .filter(|(_, key)| {
                dream_abi::intrinsics::IntrinsicOp::from_key(key)
                    == Some(dream_abi::intrinsics::IntrinsicOp::DebugRuntimeCounter)
            })
            .map(|(def, _)| ids.stable(IdSpace::Def, def.to_raw()))
            .collect::<Option<Vec<_>>>()?;
        counters.sort_unstable();
        let summary_switches =
            postcard::to_allocvec(&(FORMAT, mir.profile.is_debug(), counters)).ok()?;
        Some(Session {
            ids,
            keys,
            bodies,
            summary_switches,
            summaries: Vec::new(),
        })
    }

    /// Each function's module and cache key, or `None` for functions whose key is not unique.
    fn keys(&self, mir: &Mir) -> Vec<Option<(String, String)>> {
        let keys: Vec<Option<(String, String)>> = mir
            .functions
            .iter()
            .map(|f| (f, ""))
            .chain(mir.polls.iter().map(|f| (f, "#poll")))
            .map(|(f, suffix)| {
                let module = self.defs.module(f.def)?;
                Some((module.to_string(), format!("{}{suffix}", f.symbol)))
            })
            .collect();
        let mut counts: IndexMap<&str, usize> = IndexMap::new();
        for (_, key) in keys.iter().flatten() {
            *counts.entry(key).or_default() += 1;
        }
        let unique: IndexSet<String> = counts
            .into_iter()
            .filter(|&(_, count)| count == 1)
            .map(|(key, _)| key.to_string())
            .collect();
        keys.into_iter()
            .map(|key| key.filter(|(_, key)| unique.contains(key)))
            .collect()
    }

    fn previous(&self, key: &(String, String)) -> Option<&Entry> {
        self.previous.get(&key.0)?.get(&key.1)
    }

    /// Inserts reference counts into every function, reusing cached results where they apply.
    pub(crate) fn insert(
        &mut self,
        mut session: Session,
        mir: &mut Mir,
        tables: (&TypeInterner, &LayoutTable, &ModRefTable),
        holds: &IndexSet<DefId>,
        insert: impl Fn(&mut MirFunction) + Sync,
    ) {
        use rayon::prelude::*;
        let context = session.context(tables.2, holds, &mir.layouts);
        let summaries = std::mem::take(&mut session.summaries);
        let this = &*self;
        let session = &session;
        let inserted: Vec<_> = mir
            .functions
            .par_iter_mut()
            .chain(mir.polls.par_iter_mut())
            .enumerate()
            .map(|(index, f)| {
                let (Some(context), Some(key), Some(body)) =
                    (&context, &session.keys[index], session.bodies[index])
                else {
                    insert(f);
                    return Inserted::Uncached;
                };
                let input = combine(context, body);
                let previous = this
                    .previous(key)
                    .and_then(|entry| entry.inserted.as_ref())
                    .filter(|(key, _)| *key == input);
                session.run(f, input, previous, tables, &insert)
            })
            .collect();
        let mut summaries = summaries.into_iter();
        for (index, (key, inserted)) in session.keys.iter().zip(inserted).enumerate() {
            let summary = if index < mir.functions.len() {
                summaries.next().flatten()
            } else {
                None
            };
            let Some((module, key)) = key else {
                continue;
            };
            let inserted = match inserted {
                Inserted::Reused(entry) => {
                    self.reused += 1;
                    Some(entry)
                }
                Inserted::Stored(entry) => Some(entry),
                Inserted::Uncached => None,
            };
            if summary.is_none() && inserted.is_none() {
                continue;
            }
            self.current
                .entry(module.clone())
                .or_default()
                .insert(key.clone(), Entry { summary, inserted });
        }
        tracing::info!(functions = self.reused, "reused ownership");
    }

    /// Builds the mod/ref table, taking each function's local summary from the cache when it
    /// applies, and keeps every summary for storing.
    pub(crate) fn modref(
        &self,
        session: &mut Session,
        mir: &Mir,
        interner: &TypeInterner,
    ) -> ModRefTable {
        let cached = |index: usize, may_destroy: bool| {
            let key = session.summary_key(index, may_destroy)?;
            let (stored, bytes) = self
                .previous(session.keys[index].as_ref()?)?
                .summary
                .as_ref()?;
            (*stored == key)
                .then(|| {
                    portable::decode_stable(session.ids.clone(), || postcard::from_bytes(bytes))
                        .ok()
                })
                .flatten()
        };
        let (table, locals) = ModRefTable::compute_with(mir, interner, &cached);
        let ids: Arc<dyn StableIds> = session.ids.clone();
        session.summaries = locals
            .iter()
            .enumerate()
            .map(|(index, (may_destroy, summary))| {
                let key = session.summary_key(index, *may_destroy)?;
                let (bytes, _) =
                    portable::encode_stable(ids.clone(), || postcard::to_allocvec(summary));
                Some((key, bytes.ok()?))
            })
            .collect();
        table
    }
}

enum Inserted {
    Reused((u128, Vec<u8>)),
    Stored((u128, Vec<u8>)),
    Uncached,
}

/// The digests of one compilation's functions, in `functions` then `polls` order.
pub(crate) struct Session {
    ids: Arc<ids::Ids>,
    keys: Vec<Option<(String, String)>>,
    bodies: Vec<Option<u128>>,
    summary_switches: Vec<u8>,
    summaries: Vec<Option<(u128, Vec<u8>)>>,
}

impl Session {
    fn summary_key(&self, index: usize, may_destroy: bool) -> Option<u128> {
        let body = (*self.bodies.get(index)?)?;
        Some(ids::hash(&(body, may_destroy, &self.summary_switches)))
    }

    fn context(
        &self,
        modref: &ModRefTable,
        holds: &IndexSet<DefId>,
        layouts: &LayoutTable,
    ) -> Option<Vec<u8>> {
        let mut holds = holds
            .iter()
            .map(|def| self.ids.stable(IdSpace::Def, def.to_raw()))
            .collect::<Option<Vec<_>>>()?;
        holds.sort_unstable();
        postcard::to_allocvec(&(
            FORMAT,
            modref.switches(&*self.ids)?,
            holds,
            format!("{:?}", layouts.target),
        ))
        .ok()
    }

    /// Replaces `f` with its cached inserted body, or runs `insert` and records the result.
    fn run(
        &self,
        f: &mut MirFunction,
        input: u128,
        previous: Option<&(u128, Vec<u8>)>,
        (interner, layouts, modref): (&TypeInterner, &LayoutTable, &ModRefTable),
        insert: impl FnOnce(&mut MirFunction),
    ) -> Inserted {
        if let Some(entry) = previous
            && let Some(output) = self.reuse(&entry.1, interner, layouts, modref)
        {
            *f = output;
            return Inserted::Reused(entry.clone());
        }
        let ids: Arc<dyn StableIds> = self.ids.clone();
        let ((), answers) = log::record(ids.clone(), || insert(f));
        let Some(answers) = answers else {
            return Inserted::Uncached;
        };
        let answers: Vec<(Query, u128)> = answers.into_iter().collect();
        let (body, _) = portable::encode_stable(ids, || postcard::to_allocvec(&(answers, &*f)));
        match body {
            Ok(body) => Inserted::Stored((input, body)),
            Err(_) => Inserted::Uncached,
        }
    }

    fn reuse(
        &self,
        bytes: &[u8],
        interner: &TypeInterner,
        layouts: &LayoutTable,
        modref: &ModRefTable,
    ) -> Option<MirFunction> {
        let (answers, output): (Vec<(Query, u128)>, MirFunction) =
            portable::decode_stable(self.ids.clone(), || postcard::from_bytes(bytes)).ok()?;
        answers
            .iter()
            .all(|(query, answer)| {
                log::replay(query, modref, interner, layouts, &*self.ids) == Some(*answer)
            })
            .then_some(output)
    }
}

fn body_digest(
    f: &MirFunction,
    interner: &TypeInterner,
    layouts: &LayoutTable,
    ids: &Arc<ids::Ids>,
    facts: &facts::Facts,
) -> Option<u128> {
    let (bytes, used) = portable::encode_stable(ids.clone(), || postcard::to_allocvec(f));
    let facts = facts.digest(&used, interner, layouts, ids)?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(&bytes.ok()?);
    hasher.update(&facts.to_le_bytes());
    Some(low_bits(&hasher.finalize()))
}

fn combine(context: &[u8], body: u128) -> u128 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(context);
    hasher.update(&body.to_le_bytes());
    low_bits(&hasher.finalize())
}

fn low_bits(hash: &blake3::Hash) -> u128 {
    let mut low = [0; 16];
    low.copy_from_slice(&hash.as_bytes()[..16]);
    u128::from_le_bytes(low)
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

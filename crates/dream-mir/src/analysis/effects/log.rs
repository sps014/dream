//! Records which whole-program facts one function's ownership lowering read, so a persisted
//! result is reused only while every recorded answer is unchanged.
//!
//! Recording is scoped to the current thread: a function's lowering runs on one thread, and
//! queries made outside a [`record`] scope cost one thread-local check.

use super::{ModRef, ModRefTable};
use crate::Global;
use dream_hir::LayoutTable;
use dream_types::portable::{IdSpace, StableIds};
use dream_types::{DefId, TypeId, TypeInterner};
use indexmap::IndexMap;
use std::cell::RefCell;
use std::sync::Arc;

/// One mod/ref table lookup.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub(crate) enum Query {
    Call(DefId, Vec<TypeId>),
    Observes(DefId, Vec<TypeId>),
    ObservesCtor(DefId),
    CtorMayDestroy(DefId),
    CallMayDestroy(DefId, Vec<TypeId>),
    Ctor(DefId),
    Iface(
        #[serde(with = "dream_hir::interface_index")] usize,
        usize,
    ),
    Del,
    MayRunDel(TypeId),
    GlobalType(Global),
}

pub(crate) enum Answer<'a> {
    ModRef(&'a ModRef),
    Bool(bool),
    Type(Option<TypeId>),
}

/// Each distinct query a scope made, with its answer's digest; `None` once some answer named an
/// id without a stable identity.
pub(crate) type Log = Option<IndexMap<Query, u128>>;

struct Recorder {
    ids: Arc<dyn StableIds>,
    answers: Log,
}

thread_local! {
    static RECORDER: RefCell<Option<Recorder>> = const { RefCell::new(None) };
}

/// Runs `run` while recording its queries on this thread.
pub(crate) fn record<T>(ids: Arc<dyn StableIds>, run: impl FnOnce() -> T) -> (T, Log) {
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            RECORDER.with(|recorder| recorder.borrow_mut().take());
        }
    }
    RECORDER.with(|recorder| {
        *recorder.borrow_mut() = Some(Recorder {
            ids,
            answers: Some(IndexMap::new()),
        })
    });
    let guard = Guard;
    let value = run();
    let answers = RECORDER
        .with(|recorder| recorder.borrow_mut().take())
        .and_then(|recorder| recorder.answers);
    drop(guard);
    (value, answers)
}

pub(super) fn note(query: impl FnOnce() -> Query, answer: Answer<'_>) {
    RECORDER.with(|recorder| {
        let mut recorder = recorder.borrow_mut();
        let Some(recorder) = recorder.as_mut() else {
            return;
        };
        let Some(answers) = &mut recorder.answers else {
            return;
        };
        let query = query();
        if answers.contains_key(&query) {
            return;
        }
        match digest(&answer, &*recorder.ids) {
            Some(digest) => {
                answers.insert(query, digest);
            }
            None => recorder.answers = None,
        }
    });
}

/// The digest of what `query` answers now.
pub(crate) fn replay(
    query: &Query,
    table: &ModRefTable,
    interner: &TypeInterner,
    layouts: &LayoutTable,
    ids: &dyn StableIds,
) -> Option<u128> {
    match query {
        Query::Call(def, args) => digest(&Answer::ModRef(&table.call_def(*def, args)), ids),
        Query::Observes(def, args) => digest(&Answer::Bool(table.observes_def(*def, args)), ids),
        Query::ObservesCtor(def) => digest(&Answer::Bool(table.observes_ctor(*def)), ids),
        Query::CtorMayDestroy(def) => digest(&Answer::Bool(table.ctor_may_destroy(*def)), ids),
        Query::CallMayDestroy(def, args) => {
            digest(&Answer::Bool(table.call_may_destroy(*def, args)), ids)
        }
        Query::Ctor(def) => digest(&Answer::ModRef(&table.ctor(*def)), ids),
        Query::Iface(iface, slot) => digest(&Answer::ModRef(&table.iface(*iface, *slot)), ids),
        Query::Del => digest(&Answer::ModRef(table.del()), ids),
        Query::MayRunDel(ty) => digest(
            &Answer::Bool(table.may_run_del(*ty, interner, layouts)),
            ids,
        ),
        Query::GlobalType(global) => digest(&Answer::Type(table.global_type(*global)), ids),
    }
}

/// A digest that is equal across compilations for equal answers: ids are replaced by their
/// stable identities and id-keyed sets are ordered by them.
fn digest(answer: &Answer<'_>, ids: &dyn StableIds) -> Option<u128> {
    let ty = |ty: TypeId| ids.stable(IdSpace::Type, ty.0 as u64);
    let bytes = match answer {
        Answer::Bool(value) => postcard::to_allocvec(&(0u8, value)),
        Answer::Type(value) => {
            let value = match value {
                Some(value) => Some(ty(*value)?),
                None => None,
            };
            postcard::to_allocvec(&(1u8, value))
        }
        Answer::ModRef(ModRef::Top) => postcard::to_allocvec(&2u8),
        Answer::ModRef(ModRef::Known(known)) => {
            let mut fields = known
                .fields
                .iter()
                .map(|&(base, field)| Some((ty(base)?, field)))
                .collect::<Option<Vec<_>>>()?;
            fields.sort_unstable();
            let mut field_params = known
                .field_params
                .iter()
                .map(|(&(base, field), roots)| Some(((ty(base)?, field), roots)))
                .collect::<Option<Vec<_>>>()?;
            field_params.sort_unstable_by_key(|(key, _)| *key);
            let mut slots = known
                .slots
                .iter()
                .map(|&slot| ty(slot))
                .collect::<Option<Vec<_>>>()?;
            slots.sort_unstable();
            let mut slot_params = known
                .slot_params
                .iter()
                .map(|(&slot, writes)| Some((ty(slot)?, writes)))
                .collect::<Option<Vec<_>>>()?;
            slot_params.sort_unstable_by_key(|(key, _)| *key);
            postcard::to_allocvec(&(
                3u8,
                fields,
                field_params,
                slots,
                slot_params,
                [
                    known.any_slot,
                    known.observes_rc,
                    known.observes_heap,
                    known.may_destroy,
                ],
            ))
        }
    };
    let hash = blake3::hash(&bytes.ok()?);
    Some(u128::from_le_bytes(hash.as_bytes()[..16].try_into().ok()?))
}

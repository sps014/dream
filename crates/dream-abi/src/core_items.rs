//! Core-language declarations the compiler may name.
//!
//! Every identity is qualified (`"system.core::Option"`), so resolution hits that module and a
//! user declaration with the same short name cannot shadow it. Short names are the spellings
//! users write and the names monomorphization is built from; they are not lookups.
//!
//! Library packages (`system.json`, `system.gpu`, and the rest) are absent on purpose.

/// The method receiver.
pub const THIS_NAME: &str = "this";

pub const OPTION_SOME: &str = "Some";
pub const OPTION_NONE: &str = "None";
pub const RESULT_OK: &str = "Ok";
pub const RESULT_ERR: &str = "Err";

pub const OPTION: &str = "system.core::Option";
pub const RESULT: &str = "system.core::Result";
pub const UNIT: &str = "system.core::Unit";
pub const RANGE: &str = "system.core::Range";
pub const WEAK: &str = "system.core::Weak";
pub const CLOSURE: &str = "system.core::Closure";
pub const REF_BOX: &str = "system.core::RefBox";
pub const CAPTURE_CELL: &str = "system.core::CaptureCell";
pub const SPAN: &str = "system.core::Span";
pub const BYTES: &str = "system.core::Bytes";
pub const ARRAY: &str = "system.core::Array";
pub const SYSTEM: &str = "system.core::System";

pub const ITERABLE: &str = "system.core::Iterable";
pub const ITERATOR: &str = "system.core::Iterator";
pub const ASYNC_ITERABLE: &str = "system.core::AsyncIterable";
pub const ASYNC_ITERATOR: &str = "system.core::AsyncIterator";
pub const COLLECTION: &str = "system.core::Collection";
pub const INDEXED_COLLECTION: &str = "system.core::IndexedCollection";
pub const CHANNEL: &str = "system.core::YieldChannel";
pub const SEQUENCE: &str = "system.core::YieldIterable";
pub const ASYNC_SEQUENCE: &str = "system.core::YieldAsyncIterable";
pub const LEASE: &str = "system.core::IteratorLease";
pub const ASYNC_LEASE: &str = "system.core::AsyncIteratorLease";

pub const LIST: &str = "system.collections::List";
pub const SET: &str = "system.collections::Set";
pub const MAP: &str = "system.collections::Map";
pub const SORTED_MAP: &str = "system.collections::SortedMap";

pub const VECTOR: &str = "system.simd::Vector";

pub const C_PTR: &str = "system::CPtr";
pub const OWNED_C_PTR: &str = "system::OwnedCPtr";
pub const NATIVE_CALLBACK: &str = "system::NativeCallback";

pub const OPTION_NAME: &str = "Option";
pub const RESULT_NAME: &str = "Result";
pub const UNIT_NAME: &str = "Unit";
pub const RANGE_NAME: &str = "Range";
pub const WEAK_NAME: &str = "Weak";
pub const CLOSURE_NAME: &str = "Closure";
pub const REF_BOX_NAME: &str = "RefBox";
pub const CAPTURE_CELL_NAME: &str = "CaptureCell";
pub const ARRAY_NAME: &str = "Array";
pub const LIST_NAME: &str = "List";
pub const SET_NAME: &str = "Set";
pub const MAP_NAME: &str = "Map";
pub const VECTOR_NAME: &str = "Vector";
pub const SYSTEM_NAME: &str = "System";
pub const C_PTR_NAME: &str = "CPtr";
pub const OWNED_C_PTR_NAME: &str = "OwnedCPtr";
pub const NATIVE_CALLBACK_NAME: &str = "NativeCallback";

/// The unqualified declaration spelled by a qualified core identity.
pub fn short_name(identity: &str) -> &str {
    identity
        .rsplit_once("::")
        .map(|(_, name)| name)
        .unwrap_or(identity)
}

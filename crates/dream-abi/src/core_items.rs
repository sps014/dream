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
/// Host loop the generated `main` calls on targets whose operating system owns the main thread.
pub const PLATFORM_MAIN: &str = "system.core::platform_main";

pub const EQUATABLE: &str = "system.core::Equatable";
pub const COMPARABLE: &str = "system.core::Comparable";
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

/// The core package that declares the native interop attributes.
pub const C_INTEROP_PACKAGE: &str = "system.interop";
/// The package that declares the scalar primitives' members.
pub const PRIMITIVES_PACKAGE: &str = "system.primitives";
/// The core package that declares the `js` type and the JS interop attributes.
pub const JS_PACKAGE: &str = "system.js";
/// The package the driver generates the JS builtin bindings into.
pub const JS_BINDINGS_PACKAGE: &str = "system.js.bindings";
/// The core package that declares the generator protocol and its attributes.
pub const CODEGEN_PACKAGE: &str = "system.codegen";
/// Virtual path prefix of sources the driver generates for a package (`<std>/generated/<package>/…`).
pub const GENERATED_PREFIX: &str = "<std>/generated/";

pub const C_PTR: &str = "system::CPtr";
pub const OWNED_C_PTR: &str = "system::OwnedCPtr";
pub const NATIVE_CALLBACK: &str = "system::NativeCallback";

pub const FUTURE_NAME: &str = dream_syntax::nodes::types::FUTURE_TYPE;
pub const OPTION_NAME: &str = "Option";
pub const RESULT_NAME: &str = "Result";
pub const UNIT_NAME: &str = "Unit";
pub const RANGE_NAME: &str = "Range";
pub const WEAK_NAME: &str = "Weak";
pub const CLOSURE_NAME: &str = "Closure";
pub const REF_BOX_NAME: &str = dream_syntax::nodes::types::REF_BOX_TYPE;
pub const CAPTURE_CELL_NAME: &str = "CaptureCell";
pub const ARRAY_NAME: &str = "Array";
pub const ITERATOR_NAME: &str = "Iterator";
pub const ASYNC_ITERABLE_NAME: &str = "AsyncIterable";
pub const ASYNC_ITERATOR_NAME: &str = "AsyncIterator";
pub const LIST_NAME: &str = "List";
pub const SET_NAME: &str = "Set";
pub const MAP_NAME: &str = "Map";
pub const VECTOR_NAME: &str = "Vector";
pub const SYSTEM_NAME: &str = "System";
/// The opaque pointer struct: one `usize` field, passed to C by value.
pub const C_PTR_NAME: &str = "CPtr";
/// The class a `@c` extern with `@owned("free_fn")` returns: fields `ptr: CPtr` then `free: usize`
/// (the C function pointer its destructor calls).
pub const OWNED_C_PTR_NAME: &str = "OwnedCPtr";
/// The callback class `NativeCallback<F>`, passed to C as `(fn, void* user_data)`.
pub const NATIVE_CALLBACK_NAME: &str = "NativeCallback";

/// The unqualified declaration spelled by a qualified core identity.
pub fn short_name(identity: &str) -> &str {
    identity
        .rsplit_once("::")
        .map(|(_, name)| name)
        .unwrap_or(identity)
}

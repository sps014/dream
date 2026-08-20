//! MIR -> WAT (text WebAssembly) backend.
//!
//! The relooper ([`crate::relooper`]) recovers structured shapes from the CFG. Sync functions walk
//! that shape tree into nested WASM `block`/`loop`/`if` when every edge can be labeled; otherwise
//! they fall back to a `$__pc` + `br_table` dispatch loop. Async poll functions always keep PC
//! dispatch (suspend/resume needs a durable program counter). Straight-line statements, operands,
//! and arithmetic are emitted directly. Memory-backed places (struct fields, array elements) and
//! allocation reuse the existing runtime/object/string layers.

pub(crate) use crate::{
    BinOp, Const, MirFunction, Operand, Place, Rvalue, Statement, Terminator, UnOp,
};
use dream_hir::{scalar_size, LayoutTable};
use dream_types::{DefId, PrimTy, TyKind, TypeId, TypeInterner};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

/// Runtime type tag for arrays passed to `$malloc`, matching the object protocol's `$object_tag`
/// dispatch (see [`crate::abi::TAG_ARRAY`]).
const ARRAY_TAG: i32 = crate::abi::TAG_ARRAY;

/// The first tag assigned to a user struct/union; consecutive types get consecutive tags, so the
/// shared runtime's dispatch tables agree (see [`crate::abi::TAG_STRUCT_BASE`]).
const STRUCT_TAG_BASE: i32 = crate::abi::TAG_STRUCT_BASE;

/// The heap-block tag for strings (see [`crate::abi::TAG_STRING`]), written into the header of
/// interned string blocks so the runtime treats them as strings.
const STRING_TAG: i32 = crate::abi::TAG_STRING;

/// Byte size of the universal heap-block header `[size:i32][tag:i32][ref_count:i32]` that precedes
/// every allocated value; a value's pointer points at `block_start + HEAP_HEADER_SIZE`.
const HEAP_HEADER_SIZE: u32 = crate::abi::HEAP_HEADER_SIZE;

/// Base address (block start) of the interned string data segment. Each string is a heap-object
/// block `[size=0][tag=STRING][ref_count=1][unit_len:i32][pad:i32][utf16le]`; the mapped address
/// points at the unit_len word (block start + header), with UTF-16 units at `ptr+8`. `$str_byte_size`
/// is `unit_len * 2`; `$str_scalar_len` loads `unit_len` at `ptr`. There is no NUL terminator (the length
/// prefix makes it redundant). The heap starts above.
const STRING_BASE: u32 = crate::abi::STRING_BASE;

/// Bytes reserved for the shadow stack (inline value-`struct` locals). It grows *downward* from its
/// region top (which is also the heap base); the heap grows *upward* from there via `memory.grow`.
/// Each function with value locals reserves its frame by subtracting from `$__sp` in its prologue
/// and restores `$__sp` before every return. See [`crate::abi`] for the full memory layout.
const SHADOW_STACK_SIZE: u32 = crate::abi::SHADOW_STACK_SIZE;

/// Pages of heap mapped in the initial linear memory beyond the shadow-stack region.
const INITIAL_HEAP_PAGES: u32 = crate::abi::INITIAL_HEAP_PAGES;

/// WASM linear-memory page size, in bytes.
const WASM_PAGE_SIZE: u32 = crate::abi::WASM_PAGE_SIZE;

/// The fixed allocator runtime (`$malloc`/`$free`/`$retain`/`$release_generic`/`$object_tag`), the
/// single source of truth for the heap ABI. Its debug-counter placeholders are filled in by
/// [`runtime_prelude`] (instrumentation on in debug builds, the default; off under `--release`).
const RUNTIME_ALLOCATOR: &str = include_str!("../../runtime/allocator.wat");

/// The fixed string runtime (`$str_scalar_len`/`$str_byte_size`/`$char_at`/`$byte_at`/`$string_eq`/`$concat_strings`/…).
/// Self-contained given the allocator + memory.
const RUNTIME_STRINGS: &str = include_str!("../../runtime/strings.wat");
const RUNTIME_REGEX: &str = include_str!("../../runtime/regex.wat");
const RUNTIME_SIMD: &str = include_str!("../../runtime/simd.wat");

fn linked_runtime_wat(id: &str) -> &'static str {
    match id {
        "regex" => RUNTIME_REGEX,
        other => crate::internal_error!("no WAT for linked runtime module {other}"),
    }
}

/// The object runtime: box/unbox/hash plus the integer-family `*_to_string` formatters
/// (`$int_to_string`/`$long_to_string`/`$byte_to_string`/…). `{TAG_*}` placeholders are substituted.
const RUNTIME_OBJECT: &str = include_str!("../../runtime/object.wat");

/// The decimal `float`/`double` formatter (`$float_to_string`/`$double_to_string`). `{TAG_STRING}`
/// is substituted; the interned `"-"` pointer is `$__rt_str_minus` (defined by the emitter).
const RUNTIME_FORMAT: &str = include_str!("../../runtime/format.wat");

/// The shared `$dream_panic(msg)` runtime helper (print message + trap). Self-contained given only
/// the always-present `$print_string`/`$print_char` imports.
const RUNTIME_PANIC: &str = include_str!("../../runtime/panic.wat");
const RUNTIME_WEAK: &str = include_str!("../../runtime/weak.wat");
const RUNTIME_CLOSURE: &str = include_str!("../../runtime/closure.wat");

/// Cross-thread synchronization primitives (`$__thread_id`/`$__lock_acquire`/`$__lock_release`/
/// `$retain_shared`) backing `@shared class`, `lock (obj) { ... }`, and `Lock`. `{THREAD_ID_COUNTER_ADDR}`
/// is substituted in `module.rs` alongside the other fixed shared-memory address constants.
const RUNTIME_SYNC: &str = include_str!("../../runtime/sync.wat");
const RUNTIME_DEFER: &str = include_str!("../../runtime/defer.wat");

/// String constants the `*_to_string` runtime references by address: `bool` renders to `"true"`/
/// `"false"`; the `double` formatter prepends `"-"`. Interned into every module so the runtime is
/// always self-contained, regardless of which literals the program itself uses.
const RUNTIME_STR_CONSTS: [&str; 4] = ["", "true", "false", "-"];

/// Located panic intern strings — see [`crate::backend::shared::panic_msgs`].
pub(crate) use crate::backend::shared::panic_msgs;

mod builder;
pub mod debug_map;
mod emitter;
mod js_marshal;
mod module;
mod protocol;
mod release;
mod runtime;
mod strings;
mod tables;
mod types;
mod valuetype;
mod wasm_types;

// Flat internal re-exports so each submodule can `use super::*` and call sibling helpers
// exactly as it did when this was one file. Kept private (not part of the crate API).
use emitter::*;
use js_marshal::*;
use protocol::*;
use release::*;
use runtime::*;
use strings::*;
use tables::*;
use types::*;
use valuetype::*;
use wasm_types::*;

pub(crate) use crate::backend::shared::{func_symbol, poll_symbol};
pub(crate) use valuetype::{vs_drop_sym, vs_retain_sym};

// The external API of the backend, at the historical `crate::backend::wasm::…` paths.
pub use builder::print_wasm;
pub(crate) use builder::FuncBuilder;
pub use debug_map::DebugModule;
pub(crate) use emitter::emit_async_poll;
pub use emitter::emit_function;
pub use module::{emit_module, emit_module_bytes, emit_module_with_debug, emit_program};
pub(crate) use wasm_types::wasm_ty_of;

#[cfg(test)]
mod tests;

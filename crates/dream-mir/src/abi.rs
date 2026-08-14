//! Runtime ABI constants shared between the MIR backend and the embedded runtime `.wat` layers.
//!
//! Every heap block carries a type tag in its header (`[size][tag][reserved]`). Reference types
//! store their tag in the block they already own; primitives are boxed into a small tagged block.
//! These are the single source of truth for those tags — the `{TAG_*}` placeholders in
//! `runtime/object.wat` / `runtime/format.wat` are substituted from them at emit time, and the host
//! interop layer (`execution/host`) mirrors the same values.

pub const TAG_INT: i32 = 1;
pub const TAG_FLOAT: i32 = 2;
pub const TAG_DOUBLE: i32 = 3;
pub const TAG_BOOL: i32 = 4;
pub const TAG_STRING: i32 = 5;
pub const TAG_ARRAY: i32 = 6;
pub const TAG_CHAR: i32 = 7;
pub const TAG_LONG: i32 = 8;
pub const TAG_UINT: i32 = 9;
pub const TAG_ULONG: i32 = 10;
pub const TAG_BYTE: i32 = 11;
/// Blittable / non-ref element arrays (`int[]`, `byte[]`, …). Same `[len][elems…]` layout as
/// [`TAG_ARRAY`].
pub const TAG_FLAT_ARRAY: i32 = 12;
/// Structs/unions are assigned consecutive tags starting here, ordered by sorted type name.
pub const TAG_STRUCT_BASE: i32 = 13;

// -- Heap block layout ---------------------------------------------------------------------------
//
// Every allocated value is preceded by `[size:i32][tag:i32][reserved:i32]`. Size `0` marks
// immortal interned strings (`$free` is a no-op). See `docs/compiler/12-allocators.md`.

/// Byte size of the universal heap-block header `[size:i32][tag:i32][reserved:i32]`. A value's data
/// pointer is `block_start + HEAP_HEADER_SIZE`.
pub const HEAP_HEADER_SIZE: u32 = 12;

/// Byte size of the length/count prefix preceding an array's elements at the data pointer
/// (`[count:i32][payload...]`); the payload starts at `ptr + LEN_PREFIX_SIZE`. Also the size of the
/// first word of a string (byte_len); strings additionally store scalar_len at `ptr+4` and utf8 at
/// `ptr + STRING_UTF8_OFFSET` — see [`STRING_HEADER_SIZE`].
pub const LEN_PREFIX_SIZE: u32 = 4;

/// Byte size of a string's data header `[byte_len:i32][scalar_len:i32]` before utf8 bytes.
pub const STRING_HEADER_SIZE: u32 = 8;
/// Offset of utf8 bytes from the string data pointer.
pub const STRING_UTF8_OFFSET: u32 = 8;
/// Offset of cached scalar-length word from the string data pointer.
pub const STRING_SCALAR_LEN_OFFSET: u32 = 4;

// -- Linear memory -------------------------------------------------------------------------------
//
// Layout, low -> high address:
//
//   [ static data ] [ shadow stack -> grows DOWN ] [ heap -> grows UP ]
//   ^ strings+itables ^ SHADOW_STACK_SIZE bytes     ^ heap base == shadow-stack top
//
// Low memory holds freelist heads, the alloc lock, and the heap bump (all below [`STRING_BASE`]).

/// WASM linear-memory page size, in bytes.
pub const WASM_PAGE_SIZE: u32 = 65536;

/// Bytes reserved for the shadow stack. It occupies its own region just above the static data and
/// grows *downward*; the heap base sits at the top of this region. Sized to comfortably hold deep
/// value-`struct` recursion (overflowing it is a stack-overflow bug, not a heap/stack collision).
pub const SHADOW_STACK_SIZE: u32 = 16 * WASM_PAGE_SIZE; // 1 MiB

/// Pages of heap mapped in the initial memory, beyond the static-data + shadow-stack regions. The
/// heap grows past this on demand via `memory.grow`, so this is only a starting cushion (~2 MiB).
pub const INITIAL_HEAP_PAGES: u32 = 34;

/// Maximum page count declared on the module's linear memory. The WASM threads proposal requires a
/// shared memory to declare a fixed maximum up front (unlike a plain memory, which may leave it
/// unbounded) — this is the wasm32 address-space ceiling (`65536 * 64KiB` = 4 GiB), so it does not
/// otherwise constrain how far the bump-pointer heap (`memory.grow`) can grow.
pub const MAX_MEMORY_PAGES: u32 = 65536;

/// Base address (block start) of the interned string data segment; the heap begins above it.
pub const STRING_BASE: u32 = 1024;

// -- Cross-thread allocator (linear memory is `shared`) ------------------------------------------
//
// The segregated free-list table occupies bytes [4, 44) for size-class heads 0..8
// (`runtime/allocator.wat`), plus large-class heads at 56+ (idx 9..12) and the huge-list head at 72.
// Coordination words at 44/48/52 must not overlap freelist heads — all well below `STRING_BASE`.

/// Spinlock serializing `$malloc`/`$free` across shared-memory instances.
pub const ALLOC_LOCK_ADDR: u32 = 44;
/// Heap bump high-water mark (shared).
pub const HEAP_PTR_ADDR: u32 = 48;

/// Monotonic thread-id counter for `$__thread_id` / `@shared` lock words.
pub const THREAD_ID_COUNTER_ADDR: u32 = 52;

// -- `@shared class` header extension --------------------------------------------------------
//
// An `@shared class` instance carries one extra `i32` word, right past its last field (i.e. at
// `data_ptr + TypeLayout::size`, computed per-type at emit time — see `Rvalue::New` emission in
// `src/mir/emit/emitter/rvalue/mod.rs`), reserved as a reentrant lock word backing `lock (obj)
// { ... }` and `Lock`'s `acquire`/`release`. Packed as `(owner_thread_id << LOCK_DEPTH_BITS) |
// recursion_depth`, `0` meaning unlocked. Ordinary (non-`@shared`) classes pay nothing for this —
// their allocation size is exactly their field layout's `size`, no extra word.
pub const HEADER_LOCK_WORD_SIZE: u32 = 4;

/// Bit width of the reentrant lock word's recursion-depth field (low bits); the remaining high
/// bits hold the owning thread's id. `1 << LOCK_DEPTH_BITS` is both the max recursion depth and
/// the max distinct thread ids this scheme supports — 65536 of each is far beyond any realistic
/// nesting depth or worker-thread count.
pub const LOCK_DEPTH_BITS: u32 = 16;

// -- Runtime export / import symbol names --------------------------------------------------------
//
// The names below form the contract between the emitted module and every host (`execution/host`,
// `wasm_runner`, `runtime/dream.js`) plus the passes that special-case the entry point. Keeping
// them here means a rename is a single edit.

/// The program entry point exported to, and invoked by, the host.
pub const ENTRY_FN: &str = "main";

/// Host import module for the fixed `print_*` builtins.
pub const ENV_MODULE: &str = "env";

/// Exported allocator entry points the host uses to build heap values.
pub const EXPORT_MALLOC: &str = "malloc";
pub const EXPORT_FREE: &str = "free";
/// Exported linear memory.
pub const EXPORT_MEMORY: &str = "memory";

/// Async-runtime exports the host scheduler bridge drives (see `execution/host/http.rs` and
/// `runtime/dream.js`).
pub const EXPORT_RUN_LOOP: &str = "__dream_run_loop";
pub const EXPORT_RESOLVE: &str = "__dream_resolve";
pub const EXPORT_NEW_FUTURE: &str = "__dream_new_future";

/// Worker-thread trampoline export (see `src/stdlib/core/webworker.dream`). The *native* host
/// worker driver (`execution/host/worker.rs`) calls this with a body funcref index and a message
/// string pointer; it performs one `call_indirect` on the `fun(string): string` body — driving an
/// async body's constructor to completion in place if the call_indirect result turns out to be an
/// untagged `Future` frame rather than the real value (see `src/mir/emit/module.rs`) — and returns
/// the reply string pointer. Kept a fixed export so a freshly instantiated worker instance of the
/// same module can be driven entirely from the host. Sound only because every native host `async`
/// op resolves synchronously before returning to WASM; the browser worker driver (`runtime/dream.js`)
/// cannot assume that, so it calls [`EXPORT_WORKER_INVOKE_RAW`] instead and drives completion itself.
pub const EXPORT_WORKER_INVOKE: &str = "__dream_worker_invoke";

/// The same one `call_indirect` `__dream_worker_invoke` performs, minus its synchronous
/// drive-to-completion step — the raw constructor/function return value, un-interpreted. Used only
/// by the browser worker driver (`runtime/dream.js`), which must instead check whether the result
/// is a still-pending `Future` and await it asynchronously (a real `extern async` host call there
/// settles later via a Promise callback, never synchronously within the `call_indirect`).
pub const EXPORT_WORKER_INVOKE_RAW: &str = "__dream_worker_invoke_raw";

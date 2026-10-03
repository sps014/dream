// Authoritative guest ABI registry. Generated C and JS files must not be edited.

macro_rules! numbers {
 ($($(#[$attr:meta])* $name:ident: $ty:ty = $value:expr => $c:literal;)*) => {
 $($(#[$attr])* pub const $name: $ty = $value;)*
 pub const ABI_NUMBERS: &[(&str, i64)] = &[$(($c, $name as i64)),*];
 };
}
numbers! {
TAG_INT: i32 = 1 => "TAG_INT";
TAG_FLOAT: i32 = 2 => "TAG_FLOAT";
TAG_DOUBLE: i32 = 3 => "TAG_DOUBLE";
TAG_BOOL: i32 = 4 => "TAG_BOOL";
TAG_STRING: i32 = 5 => "TAG_STRING";
TAG_ARRAY: i32 = 6 => "TAG_ARRAY";
TAG_CHAR: i32 = 7 => "TAG_CHAR";
TAG_LONG: i32 = 8 => "TAG_LONG";
TAG_UINT: i32 = 9 => "TAG_UINT";
TAG_ULONG: i32 = 10 => "TAG_ULONG";
TAG_BYTE: i32 = 11 => "TAG_BYTE";
TAG_ISIZE: i32 = 12 => "TAG_ISIZE";
TAG_USIZE: i32 = 13 => "TAG_USIZE";
/// Coroutine `Future` frames (`dream_new_future`). Distinct from tag 0 (untagged C/weak blocks)
/// so last-release can run the per-poll slot destructor.
TAG_FUTURE: i32 = 256 => "TAG_FUTURE";
/// First-class `fun` value (`dream_funcbox_new`). Distinct from `TAG_STRUCT_BASE` so
/// `dream_release_object` does not run the first struct's destructor on a funcbox.
TAG_FUNCBOX: i32 = 257 => "TAG_FUNCBOX";
/// Multi-capture closure env (`object[]` of CaptureCells). `TAG_ARRAY` last-drop is a shallow
/// free; this tag dispatches to typed `release_array_t{object}` from `dream_release_object`.
TAG_CLOSURE_ENV: i32 = 258 => "TAG_CLOSURE_ENV";
/// Structs/unions are assigned consecutive tags starting here, ordered by sorted type name.
TAG_STRUCT_BASE: i32 = 14 => "TAG_STRUCT_BASE";
/// Header tag high bit: object is concurrently refcounted (`@shared` / published worker
/// env and wire / foreign futures). Mask with [`TAG_VALUE_MASK`] before comparing type tags.
TAG_SHARED: i32 = 1 << 30 => "TAG_SHARED";
TAG_VALUE_MASK: i32 = TAG_SHARED - 1 => "TAG_VALUE_MASK";
/// Byte size of the universal heap-block header `[size:i32][tag:i32][ref_count:i32]`. A value's data
/// pointer is `block_start + HEAP_HEADER_SIZE`.
HEAP_HEADER_SIZE: u32 = 12 => "HEAP_HEADER_SIZE";
/// Written into cleared `unowned` slots so loads can report "target destroyed" distinctly
/// from "never assigned". Mirrors `DREAM_UNOWNED_POISON` in dream_abi.h.
UNOWNED_POISON: i32 = -165764356 => "DREAM_UNOWNED_POISON";
/// Byte offset (from the block start) of the type-tag word in the heap header.
HEADER_TAG_OFFSET: u32 = 4 => "HEADER_TAG_OFFSET";
/// Byte offset (from the block start) of the reference-count word in the heap header.
HEADER_REFCOUNT_OFFSET: u32 = 8 => "HEADER_REFCOUNT_OFFSET";
/// Byte size of the length/count prefix preceding an array's elements at the data pointer
/// (`[count:i32][payload...]`); the payload starts at `ptr + LEN_PREFIX_SIZE`. Also the size of the
/// first word of a string (`unit_len`); UTF-16 payload starts at `ptr + STRING_UNITS_OFFSET`.
LEN_PREFIX_SIZE: u32 = 4 => "LEN_PREFIX_SIZE";
/// Byte size of a string's data header `[unit_len:i32][pad:i32]` before UTF-16 LE units.
STRING_HEADER_SIZE: u32 = 8 => "STRING_HEADER_SIZE";
/// Offset of UTF-16 LE units from the string data pointer when the pad word is inline (`0`).
STRING_UNITS_OFFSET: u32 = 8 => "STRING_UNITS_OFFSET";
/// Pad word at `ptr + 4`. [`DREAM_STR_SLICE`] = fat slice (`parent:dream_ptr` at +8,
/// `units:*u16` at +8+ptr_size); anything else = inline units at [`STRING_UNITS_OFFSET`], with
/// the pad caching [`string_hash`] ([`DREAM_STR_PAD_INLINE`] = not computed yet).
STRING_SCALAR_LEN_OFFSET: u32 = 4 => "STRING_SCALAR_LEN_OFFSET";
/// Pad value for an owned inline UTF-16 payload whose hash has not been cached.
DREAM_STR_PAD_INLINE: i32 = 0 => "DREAM_STR_PAD_INLINE";
/// Pad value marking a fat slice (parent + external units pointer).
DREAM_STR_SLICE: i32 = 1 => "DREAM_STR_SLICE";
/// Native malloc header `[size:usize][padding][magic:i32][tag:i32][rc:i32]`. WASM uses [`HEAP_HEADER_SIZE`].
NATIVE_HEAP_HEADER_SIZE: u32 = 32 => "NATIVE_HEAP_HEADER_SIZE";
/// `data_ptr - RC_FROM_DATA` is the refcount word ([`HEADER_REFCOUNT_OFFSET`] from block start).
RC_FROM_DATA: u32 = HEAP_HEADER_SIZE - HEADER_REFCOUNT_OFFSET => "RC_FROM_DATA";
/// `data_ptr - TAG_FROM_DATA` is the type-tag word ([`HEADER_TAG_OFFSET`] from block start).
TAG_FROM_DATA: u32 = HEAP_HEADER_SIZE - HEADER_TAG_OFFSET => "TAG_FROM_DATA";
/// WASM linear-memory page size, in bytes.
WASM_PAGE_SIZE: u32 = 65536 => "WASM_PAGE_SIZE";
/// A bounded shadow stack keeps inline-value recursion separate from heap growth.
SHADOW_STACK_SIZE: u32 = 16 * WASM_PAGE_SIZE => "SHADOW_STACK_SIZE";
/// Pages of heap mapped in the initial memory, beyond the static-data + shadow-stack regions. The
/// heap grows past this on demand via `memory.grow`, so this is only a starting cushion.
INITIAL_HEAP_PAGES: u32 = 1 => "INITIAL_HEAP_PAGES";
/// Maximum page count declared on the module's linear memory. The WASM threads proposal requires a
/// shared memory to declare a fixed maximum up front (unlike a plain memory, which may leave it
/// unbounded) — this is the wasm32 address-space ceiling (`65536 * 64KiB` = 4 GiB), so it does not
/// otherwise constrain how far the bump-pointer heap (`memory.grow`) can grow.
MAX_MEMORY_PAGES: u32 = 65536 => "MAX_MEMORY_PAGES";
/// Base address (block start) of the interned string data segment; the heap begins above it.
STRING_BASE: u32 = 1024 => "STRING_BASE";
/// `RegexFlags.IgnoreCase` / `Multiline` / `DotAll` — lockstep with `dream_abi.h` and
/// `regex_flags.dream`.
DREAM_REGEX_IGNORE_CASE: i32 = 2 => "DREAM_REGEX_IGNORE_CASE";
DREAM_REGEX_MULTILINE: i32 = 4 => "DREAM_REGEX_MULTILINE";
DREAM_REGEX_DOTALL: i32 = 8 => "DREAM_REGEX_DOTALL";
ALLOC_LOCK_ADDR: u32 = 44 => "ALLOC_LOCK_ADDR";
HEAP_PTR_ADDR: u32 = 48 => "HEAP_PTR_ADDR";
/// A monotonically increasing counter (`i32.atomic.rmw.add`) handing out a small, dense, unique id
/// to each thread that ever calls `$__thread_id` (see `runtime/sync.wat`) — the owner instance and
/// every `Task` thread draw from this one shared word, so ids never collide across threads.
/// Each thread caches its own id in the ordinary (per-*instance*) WASM global `$__tid` after the
/// first call, so every later call is a single `global.get`, not a repeat atomic RMW. Backs the
/// owner-thread-id half of the reentrant lock word (`@shared class`'s embedded lock, `lock (obj)
/// { ... }`, and `Lock`) — see `HEADER_LOCK_WORD_SIZE` below for the lock word's own layout.
THREAD_ID_COUNTER_ADDR: u32 = 52 => "THREAD_ID_COUNTER_ADDR";
/// Shared async run-queue / timer list heads. WASM globals are per-instance; these live in
/// shared linear memory so workers and the owner see the same scheduler lists.
ASYNC_RQ_HEAD_ADDR: u32 = 76 => "ASYNC_RQ_HEAD_ADDR";
ASYNC_RQ_TAIL_ADDR: u32 = 80 => "ASYNC_RQ_TAIL_ADDR";
ASYNC_TIMER_HEAD_ADDR: u32 = 84 => "ASYNC_TIMER_HEAD_ADDR";
ASYNC_VCLOCK_ADDR: u32 = 88 => "ASYNC_VCLOCK_ADDR";
HEADER_LOCK_WORD_SIZE: u32 = 4 => "HEADER_LOCK_WORD_SIZE";
/// Bit width of the reentrant lock word's recursion-depth field (low bits); the remaining high
/// bits hold the owning thread's id. `1 << LOCK_DEPTH_BITS` is both the max recursion depth and
/// the max distinct thread ids this scheme supports — 65536 of each is far beyond any realistic
/// nesting depth or worker-thread count.
LOCK_DEPTH_BITS: u32 = 16 => "LOCK_DEPTH_BITS";
FUTURE_KIND_TASK: i32 = 0 => "FUTURE_KIND_TASK";
FUTURE_KIND_HOST: i32 = 1 => "FUTURE_KIND_HOST";
FUTURE_KIND_ALL: i32 = 2 => "FUTURE_KIND_ALL";
FUTURE_KIND_ANY: i32 = 3 => "FUTURE_KIND_ANY";
FUTURE_STATUS_PENDING: i32 = 0 => "FUTURE_STATUS_PENDING";
FUTURE_STATUS_READY: i32 = 1 => "FUTURE_STATUS_READY";
FUTURE_STATUS_CANCELLED: i32 = 2 => "FUTURE_STATUS_CANCELLED";
HOST_POLL_INDEX: i32 = -1 => "HOST_POLL_INDEX";
DREAM_TAG_WEAK_TARGET: i32 = i32::MIN => "DREAM_TAG_WEAK_TARGET";
DREAM_RC_SHARED_BIT: i32 = i32::MIN => "DREAM_RC_SHARED_BIT";
DREAM_RC_IMMORTAL: i32 = DREAM_RC_SHARED_BIT => "DREAM_RC_IMMORTAL";
}

macro_rules! symbols {
 ($($(#[$attr:meta])* $name:ident: $ty:ty = $value:expr => $c:literal;)*) => {
 $($(#[$attr])* pub const $name: $ty = $value;)*
 pub const ABI_SYMBOLS: &[(&str, &str)] = &[$(($c, $name)),*];
 };
}
symbols! {
/// The program entry point exported to, and invoked by, the host.
ENTRY_FN: &str = "main" => "DREAM_SYM_ENTRY_FN";
/// The symbol that wraps [`ENTRY_FN`]; wasm32 exports it under the name [`ENTRY_FN`].
GUEST_ENTRY_FN: &str = "dream_guest_entry" => "DREAM_SYM_GUEST_ENTRY_FN";
/// Host import module for the fixed `print_*` builtins.
ENV_MODULE: &str = "env" => "DREAM_MODULE_ENV";
PRINT_STRING: &str = "print_string" => "DREAM_SYM_PRINT_STRING";
PRINT_INT: &str = "print_int" => "DREAM_SYM_PRINT_INT";
PRINT_FLOAT: &str = "print_float" => "DREAM_SYM_PRINT_FLOAT";
PRINT_DOUBLE: &str = "print_double" => "DREAM_SYM_PRINT_DOUBLE";
PRINT_CHAR: &str = "print_char" => "DREAM_SYM_PRINT_CHAR";
/// Diagnostic stream: panics and a failing `main`'s `Error:` line. Kept off stdout so program
/// output stays clean and pipeable.
PRINT_ERR_STRING: &str = "print_err_string" => "DREAM_SYM_PRINT_ERR_STRING";
PRINT_ERR_CHAR: &str = "print_err_char" => "DREAM_SYM_PRINT_ERR_CHAR";
/// Exported allocator entry points the host uses to build heap values.
EXPORT_MALLOC: &str = "malloc" => "DREAM_SYM_MALLOC";
EXPORT_FREE: &str = "free" => "DREAM_SYM_FREE";
/// Exported linear memory.
EXPORT_MEMORY: &str = "memory" => "DREAM_SYM_MEMORY";
/// Async-runtime exports the host scheduler bridge drives (see `execution/host/http.rs` and
/// `runtime/dream.js`).
EXPORT_RUN_LOOP: &str = "__dream_run_loop" => "DREAM_SYM_RUN_LOOP";
EXPORT_RESOLVE: &str = "__dream_resolve" => "DREAM_SYM_RESOLVE";
EXPORT_NEW_FUTURE: &str = "__dream_new_future" => "DREAM_SYM_NEW_FUTURE";
/// Per-instance startup (function table, heap bump, `__dream_init`). WAT uses `(start)`; C wasm32
/// exports this so `load()` and worker instances can run it without calling `main`.
EXPORT_RUNTIME_INIT: &str = "__runtime_init" => "DREAM_SYM_RUNTIME_INIT";
/// Worker-thread trampoline export (see `crates/dream-stdlib/src/system/task/task.dream`). The *native* host
/// worker driver (`execution/host/worker.rs`) calls this with a body funcref index and a message
/// string pointer; it performs one `call_indirect` on the `fun(string): string` body — driving an
/// async body's constructor to completion in place if the call_indirect result turns out to be an
/// `TAG_FUTURE` frame rather than the real value (see `src/mir/emit/module.rs`) — and returns
/// the reply string pointer. Kept a fixed export so a freshly instantiated worker instance of the
/// same module can be driven entirely from the host. Sound only because every native host `async`
/// op resolves synchronously before returning to WASM; the browser worker driver (`runtime/dream.js`)
/// cannot assume that, so it calls [`EXPORT_WORKER_INVOKE_RAW`] instead and drives completion itself.
EXPORT_WORKER_INVOKE: &str = "__dream_worker_invoke" => "DREAM_SYM_EXPORT_WORKER_INVOKE";
/// The same one `call_indirect` `__dream_worker_invoke` performs, minus its synchronous
/// drive-to-completion step — the raw constructor/function return value, un-interpreted. Used only
/// by the browser worker driver (`runtime/dream.js`), which must instead check whether the result
/// is a still-pending `Future` and await it asynchronously (a real `extern async` host call there
/// settles later via a Promise callback, never synchronously within the `call_indirect`).
EXPORT_WORKER_INVOKE_RAW: &str = "__dream_worker_invoke_raw" => "DREAM_SYM_EXPORT_WORKER_INVOKE_RAW";
/// Wasm32 export of `dream_drop_globals`. Native runs that from `dream_guest_entry`; wasm32
/// returns to JS first (async `main` still holds a Future), so `DreamInstance.run` calls this
/// after the Future settles.
EXPORT_DROP_GLOBALS: &str = "__dream_drop_globals" => "DREAM_SYM_EXPORT_DROP_GLOBALS";
/// Wasm32 export answering `main`'s process exit status, and reporting a failing `Result` main's
/// error on stderr. Takes the settled Future of an async `main` (null for a sync one): the entry's
/// own return slot already means "0, or a Future pointer", so the status needs its own channel.
EXPORT_MAIN_REPORT: &str = "__dream_main_report" => "DREAM_SYM_EXPORT_MAIN_REPORT";
/// C-backend wasm32 export mapping a `dream_ft[]` dispatch index to the function-pointer value.
/// Clang assigns `__indirect_function_table` slots independently of `dream_ft[]` order, but on
/// wasm32 a function pointer *is* its table index — so the JS host translates through this before
/// `table.get` when wrapping a FUNC-slot callback (the WAT backend needs no such export).
EXPORT_FT_GET: &str = "dream_ft_get" => "DREAM_SYM_EXPORT_FT_GET";
HOST_MODULE: &str = "Dream" => "DREAM_MODULE_HOST";
TIME_NOW_NANOS: &str = "timeNowNanos" => "DREAM_SYM_TIME_NOW_NANOS";
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FutureLayout {
    pub state: u32,
    pub status: u32,
    pub result: u32,
    pub poll: u32,
    pub waker: u32,
    pub awaiting: u32,
    pub kind: u32,
    pub children: u32,
    pub count: u32,
    pub remaining: u32,
    pub results: u32,
    pub next: u32,
    pub queued: u32,
    pub due: u32,
    /// Combinator element size.
    pub esize: u32,
    pub wide: u32,
    /// Start of saved locals. Host-allocated futures are exactly this many bytes.
    pub slots: u32,
}

impl FutureLayout {
    /// Historical wasm32 packing: every header field is an `i32`, `F_WIDE` is 8 bytes at 56,
    /// locals start at 64. Kept as a `const` so WAT/JS/host stay byte-stable.
    pub const WASM32: Self = Self::compute(4, 4, false);

    pub const fn compute(ptr_size: u32, ptr_align: u32, native: bool) -> Self {
        let mut c = 0u32;
        let state = place(&mut c, 4, 4);
        let status = place(&mut c, 4, 4);
        let result = place(&mut c, ptr_size, ptr_align);
        let poll = place(&mut c, 4, 4);
        let waker = place(&mut c, ptr_size, ptr_align);
        let awaiting = place(&mut c, ptr_size, ptr_align);
        let kind = place(&mut c, 4, 4);
        let children = place(&mut c, ptr_size, ptr_align);
        let count = place(&mut c, 4, 4);
        let remaining = place(&mut c, 4, 4);
        let results = place(&mut c, ptr_size, ptr_align);
        let next = place(&mut c, ptr_size, ptr_align);
        let queued = place(&mut c, 4, 4);
        let due = place(&mut c, 4, 4);
        let esize = if native { place(&mut c, 4, 4) } else { 0 };
        let wide = place(&mut c, 8, 8);
        let slots = align_up(c, 8);
        Self {
            state,
            status,
            result,
            poll,
            waker,
            awaiting,
            kind,
            children,
            count,
            remaining,
            results,
            next,
            queued,
            due,
            esize,
            wide,
            slots,
        }
    }
}

const fn align_up(offset: u32, align: u32) -> u32 {
    let rem = offset % align;
    if rem == 0 {
        offset
    } else {
        offset + (align - rem)
    }
}

const fn place(cursor: &mut u32, size: u32, align: u32) -> u32 {
    let off = align_up(*cursor, align);
    *cursor = off + size;
    off
}

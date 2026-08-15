//! Runtime ABI constants shared between the MIR backend and the embedded runtime `.wat` layers.
//!
//! Every heap block carries a type tag in its header (`[size][tag][gc_meta]`). Reference types
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
/// [`TAG_ARRAY`], but Gen0 must not scan elements as pointers.
pub const TAG_FLAT_ARRAY: i32 = 12;
/// Structs/unions are assigned consecutive tags starting here, ordered by sorted type name.
pub const TAG_STRUCT_BASE: i32 = 13;

// -- Heap block layout ---------------------------------------------------------------------------
//
// Every managed value is preceded by a fixed header `[size:i32][tag:i32][gc_meta:i32]`. These
// offsets are the single source of truth shared by the emitter, the host interop layer
// (`execution/host`), and the hand-written runtime `.wat` (which references them via `{...}`
// placeholders substituted at emit time, or via matching comments). See
// `docs/compiler/12-tiered-gc.md`.

/// Byte size of the universal heap-block header `[size:i32][tag:i32][gc_meta:i32]`. A value's data
/// pointer is `block_start + HEAP_HEADER_SIZE`.
pub const HEAP_HEADER_SIZE: u32 = 12;

/// Byte offset (from the block start) of the type-tag word in the heap header.
pub const HEADER_TAG_OFFSET: u32 = 4;

/// Byte offset (from the block start) of the GC metadata word in the heap header.
pub const HEADER_GC_META_OFFSET: u32 = 8;

// -- `gc_meta` bit layout ------------------------------------------------------------------------

/// Generation field mask (bits 0–1): 0=Gen0, 1=Gen1, 2=Gen2, 3=LOH.
pub const GC_META_GEN_MASK: u32 = 0b11;
pub const GC_GEN0: u32 = 0;
pub const GC_GEN1: u32 = 1;
pub const GC_GEN2: u32 = 2;
pub const GC_GEN_LOH: u32 = 3;

/// Object is marked reachable in the current collection.
pub const GC_META_MARK: u32 = 1 << 2;
/// Object was evacuated. Gen0 stows the new data pointer in the tag word; old-space compact
/// stows it in the size word. The FORWARDED bit is set in either case.
pub const GC_META_FORWARDED: u32 = 1 << 3;
/// Type has a `del` finalizer; enqueue when found unreachable.
pub const GC_META_FINALIZE: u32 = 1 << 4;
/// Finalizer already ran once (blocks resurrection loops).
pub const GC_META_FINALIZED: u32 = 1 << 5;
/// Immortal / non-movable (interned strings): never evacuated or swept.
pub const GC_META_IMMORTAL: u32 = 1 << 6;
/// Block is on the segregated freelist (old/LOH sweep); skip in heap walks.
pub const GC_META_FREE: u32 = 1 << 7;

/// Payload size (bytes) at or above which `$malloc` uses the LOH path instead of Gen0.
/// Matches C#'s ~85 KiB large-object threshold.
pub const LOH_THRESHOLD: u32 = 85 * 1024;

/// Gen0 nursery size in bytes (fixed region at heap base; copying collector).
/// 2 MiB keeps typical microbench churn (substring / concat) inside one nursery
/// so Gen0 does not run mid-loop; still small enough for web/gamedev pauses.
pub const NURSERY_SIZE: u32 = 2 * 1024 * 1024;

/// Maximum number of GC root slots in the shadow root table (scanned at safepoints).
pub const GC_ROOT_TABLE_CAP: u32 = 4096;

/// Maximum remembered-set entries (older→younger slots). Further old→young stores set
/// [`GC_REMSET_OVERFLOW_ADDR`] and [`GC_REQUEST_ADDR`]; `$malloc` then runs Gen0 (and old-space
/// mark-sweep when overflow is set or [`GC_OLD_BYTES_ADDR`] meets [`GC_GEN1_THRESHOLD`]).
pub const GC_REMEMBERED_CAP: u32 = 65536;

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
//   [ static data ] [ shadow stack -> grows DOWN ] [ nursery | old+LOH -> grows UP ]
//   ^ strings+itables ^ SHADOW_STACK_SIZE bytes     ^ heap base == shadow-stack top
//
// Low memory also holds freelist heads, alloc lock, heap bump, GC coordination words, and the
// root / remembered-set tables (all below [`STRING_BASE`]).

/// WASM linear-memory page size, in bytes.
pub const WASM_PAGE_SIZE: u32 = 65536;

/// Bytes reserved for the shadow stack. It occupies its own region just above the static data and
/// grows *downward*; the heap base sits at the top of this region. Sized to comfortably hold deep
/// value-`struct` recursion (overflowing it is a stack-overflow bug, not a heap/stack collision).
pub const SHADOW_STACK_SIZE: u32 = 16 * WASM_PAGE_SIZE; // 1 MiB

/// Pages of heap mapped in the initial memory, beyond the static-data + shadow-stack regions. The
/// heap grows past this on demand via `memory.grow`, so this is only a starting cushion.
/// Sized to cover the nursery plus a small old-space cushion.
pub const INITIAL_HEAP_PAGES: u32 = (NURSERY_SIZE / WASM_PAGE_SIZE) + 2;

/// Maximum page count declared on the module's linear memory. The WASM threads proposal requires a
/// shared memory to declare a fixed maximum up front (unlike a plain memory, which may leave it
/// unbounded) — this is the wasm32 address-space ceiling (`65536 * 64KiB` = 4 GiB), so it does not
/// otherwise constrain how far the bump-pointer heap (`memory.grow`) can grow.
pub const MAX_MEMORY_PAGES: u32 = 65536;

/// Base address (block start) of the interned string data segment; the heap begins above it.
pub const STRING_BASE: u32 = 1024;

// -- Cross-thread allocator / GC coordination (linear memory is `shared`) ------------------------
//
// The segregated free-list table occupies bytes [4, 44) for size-class heads 0..8
// (`runtime/allocator.wat`), plus large-class heads at 56+ (idx 9..12) and the huge-list head at 72.
// Coordination words at 44/48/52 must not overlap freelist heads — all well below `STRING_BASE`.
//
// GC state words occupy [76, 160) (still below STRING_BASE=1024):
// nursery bounds, old-space bump mirror, root table, remembered set, collection epoch.

/// Spinlock serializing `$malloc`/`$free`/`$gc_collect_*` across shared-memory instances.
pub const ALLOC_LOCK_ADDR: u32 = 44;
/// Old-space / LOH bump high-water mark (shared); Gen0 uses the nursery bump instead.
pub const HEAP_PTR_ADDR: u32 = 48;

/// Monotonic thread-id counter for `$__thread_id` / `@shared` lock words.
pub const THREAD_ID_COUNTER_ADDR: u32 = 52;

/// Nursery bump pointer (shared). Allocations with payload < [`LOH_THRESHOLD`] bump here first.
pub const NURSERY_BUMP_ADDR: u32 = 76;
/// Absolute nursery start (set once in `$__runtime_init` to heap_base).
pub const NURSERY_START_ADDR: u32 = 80;
/// Absolute nursery end (= start + [`NURSERY_SIZE`]).
pub const NURSERY_END_ADDR: u32 = 84;
/// Old-space start (= nursery end); Gen1/Gen2/LOH live at or above this.
pub const OLD_START_ADDR: u32 = 88;

/// Non-zero while a STW collection is in progress or has been requested.
pub const GC_REQUEST_ADDR: u32 = 92;
/// Reserved (was a multi-mutator safepoint handshake; single-threaded STW does not use it).
pub const GC_SAFEPOINT_EXPECT_ADDR: u32 = 96;
/// Reserved (was a multi-mutator safepoint handshake; single-threaded STW does not use it).
pub const GC_SAFEPOINT_ACK_ADDR: u32 = 100;
/// Collection kind: 0=ephemeral(Gen0), 1=Gen0+1, 2=full(+Gen2+LOH).
pub const GC_COLLECT_KIND_ADDR: u32 = 104;

/// Root-table count (number of live slots).
pub const GC_ROOT_COUNT_ADDR: u32 = 108;
/// Pointer to the root table (array of i32 data pointers); allocated in `$__runtime_init`.
pub const GC_ROOT_TABLE_PTR_ADDR: u32 = 124;

/// Remembered-set count.
pub const GC_REMSET_COUNT_ADDR: u32 = 112;
/// Pointer to the remembered-set table (slot addresses); allocated in `$__runtime_init`.
pub const GC_REMSET_TABLE_PTR_ADDR: u32 = 128;

/// Finalizer queue head (data pointer linked via a side list, or 0).
pub const GC_FINALIZER_HEAD_ADDR: u32 = 116;

/// Bytes allocated into old/LOH since last Gen1+ collection (trigger heuristic).
pub const GC_OLD_BYTES_ADDR: u32 = 120;
/// Gen1 collection threshold in bytes of old-space growth.
pub const GC_GEN1_THRESHOLD: u32 = 2 * 1024 * 1024;

/// Mark-stack bump for older-gen tracing (pointer into a scratch region allocated at init).
pub const GC_MARK_STACK_PTR_ADDR: u32 = 132;
pub const GC_MARK_STACK_BASE_ADDR: u32 = 136;
/// Capacity of the mark stack in entries.
pub const GC_MARK_STACK_CAP: u32 = 8192;

/// Non-zero when the remembered set hit [`GC_REMEMBERED_CAP`] and dropped further entries;
/// next Gen0 scans dirty cards (or all live old/LOH if the heap exceeds card coverage).
pub const GC_REMSET_OVERFLOW_ADDR: u32 = 140;
/// Monotonic collection epoch. Bumped at the end of every Gen0 / old collect so mutators can
/// skip root reloads when no collection ran since the last safepoint check.
pub const GC_EPOCH_ADDR: u32 = 144;
/// Pointer to the old-space card table (one byte per [`GC_CARD_SIZE`] bytes of old/LOH).
pub const GC_CARD_TABLE_PTR_ADDR: u32 = 148;
/// Atomic fetch-add counter assigning each module instance a disjoint slice of the
/// shadow-stack region. `$__sp` is per-instance, but the bytes it addresses live in
/// *shared* linear memory — without lanes, a worker's frames overwrite the owner's.
pub const SHADOW_STACK_LANE_ADDR: u32 = 152;
/// Bytes reserved for one instance's shadow stack (grows down from that lane's top).
pub const SHADOW_STACK_LANE_SIZE: u32 = 4 * WASM_PAGE_SIZE;
/// Shared async run-queue / timer list heads. WASM globals are per-instance; workers
/// collect the owner's nursery objects, so these must live in shared linear memory.
pub const ASYNC_RQ_HEAD_ADDR: u32 = 156;
pub const ASYNC_RQ_TAIL_ADDR: u32 = 160;
pub const ASYNC_TIMER_HEAD_ADDR: u32 = 164;
pub const ASYNC_VCLOCK_ADDR: u32 = 168;

/// Card size in bytes (`1 << GC_CARD_SHIFT`).
pub const GC_CARD_SHIFT: u32 = 9;
pub const GC_CARD_SIZE: u32 = 1 << GC_CARD_SHIFT;
/// Card-table length in bytes (also the number of cards). Coverage is
/// `GC_CARD_TABLE_BYTES << GC_CARD_SHIFT` (~32 MiB of old heap).
pub const GC_CARD_TABLE_BYTES: u32 = 65536;

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

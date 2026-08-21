//! Runtime ABI constants shared between the MIR backend and the embedded runtime `.wat` layers.
//!
//! Every heap block carries a type tag in its header (`[size][tag][ref_count]`). Reference types
//! store their tag in the block they already own; primitives are boxed into a small tagged block.
//! These are the single source of truth for those tags — the `{TAG_*}` placeholders in
//! `runtime/object.wat` / `runtime/format.wat` are substituted from them at emit time; interned
//! `true`/`false`/`"-"`/`""` are `$__rt_str_*` globals. The host interop layer (`execution/host`)
//! mirrors the same tag values.

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
/// Structs/unions are assigned consecutive tags starting here, ordered by sorted type name.
pub const TAG_STRUCT_BASE: i32 = 12;

// -- Heap block layout ---------------------------------------------------------------------------
//
// Every allocated value is preceded by a fixed header `[size:i32][tag:i32][ref_count:i32]`. These
// offsets are the single source of truth shared by the emitter, the host interop layer
// (`execution/host`), and the hand-written runtime `.wat` (which references them via `{...}`
// placeholders substituted at emit time, or via matching comments).

/// Byte size of the universal heap-block header `[size:i32][tag:i32][ref_count:i32]`. A value's data
/// pointer is `block_start + HEAP_HEADER_SIZE`.
pub const HEAP_HEADER_SIZE: u32 = 12;

/// Byte offset (from the block start) of the type-tag word in the heap header.
pub const HEADER_TAG_OFFSET: u32 = 4;

/// Byte offset (from the block start) of the reference-count word in the heap header.
pub const HEADER_REFCOUNT_OFFSET: u32 = 8;

/// Byte size of the length/count prefix preceding an array's elements at the data pointer
/// (`[count:i32][payload...]`); the payload starts at `ptr + LEN_PREFIX_SIZE`. Also the size of the
/// first word of a string (`unit_len`); UTF-16 payload starts at `ptr + STRING_UNITS_OFFSET`.
pub const LEN_PREFIX_SIZE: u32 = 4;

/// Byte size of a string's data header `[unit_len:i32][pad:i32]` before UTF-16 LE units.
pub const STRING_HEADER_SIZE: u32 = 8;
/// Offset of UTF-16 LE units from the string data pointer when the pad word is inline (`0`).
pub const STRING_UNITS_OFFSET: u32 = 8;
/// Pad word at `ptr + 4`. WASM: `0` = units at [`STRING_UNITS_OFFSET`]; nonzero = i32 payload
/// address. Native: [`DREAM_STR_PAD_INLINE`] = inline units; [`DREAM_STR_SLICE`] = fat slice
/// (`parent:dream_ptr` at +8, `units:*u16` at +8+ptr_size).
pub const STRING_SCALAR_LEN_OFFSET: u32 = 4;
/// Native/WASM pad value for an owned inline UTF-16 payload.
pub const DREAM_STR_PAD_INLINE: i32 = 0;
/// Native-only pad value: fat slice (parent + external units pointer). Never used as a WASM
/// payload address (those are interned `mapped_ptr + STRING_UNITS_OFFSET`, always `>= STRING_BASE`).
pub const DREAM_STR_SLICE: i32 = 1;
/// Native malloc header `[size:i32][magic:i32][tag:i32][rc:i32]`. WASM uses [`HEAP_HEADER_SIZE`].
pub const NATIVE_HEAP_HEADER_SIZE: u32 = 16;
/// `data_ptr - RC_FROM_DATA` is the refcount word ([`HEADER_REFCOUNT_OFFSET`] from block start).
pub const RC_FROM_DATA: u32 = HEAP_HEADER_SIZE - HEADER_REFCOUNT_OFFSET;
/// `data_ptr - TAG_FROM_DATA` is the type-tag word ([`HEADER_TAG_OFFSET`] from block start).
pub const TAG_FROM_DATA: u32 = HEAP_HEADER_SIZE - HEADER_TAG_OFFSET;

// -- Linear memory -------------------------------------------------------------------------------
//
// Layout, low -> high address:
//
//   [ static data ] [ shadow stack -> grows DOWN ] [ heap -> grows UP (memory.grow) ]
//   ^ strings+itables ^ SHADOW_STACK_SIZE bytes     ^ heap base == shadow-stack top
//
// The shadow stack (inline value-`struct` locals) and the heap share a single boundary and grow
// away from it in *opposite* directions, so they can never collide. The shadow stack is capped at
// `SHADOW_STACK_SIZE` (a deep-recursion bound); the heap is effectively unbounded, extending linear
// memory via `memory.grow` in the allocator's bump path.

/// WASM linear-memory page size, in bytes.
pub const WASM_PAGE_SIZE: u32 = 65536;

/// Bytes reserved for the shadow stack. It occupies its own region just above the static data and
/// grows *downward*; the heap base sits at the top of this region. Sized to comfortably hold deep
/// value-`struct` recursion (overflowing it is a stack-overflow bug, not a heap/stack collision).
pub const SHADOW_STACK_SIZE: u32 = 16 * WASM_PAGE_SIZE; // 1 MiB

/// Pages of heap mapped in the initial memory, beyond the static-data + shadow-stack regions. The
/// heap grows past this on demand via `memory.grow`, so this is only a starting cushion.
pub const INITIAL_HEAP_PAGES: u32 = 1;

/// Maximum page count declared on the module's linear memory. The WASM threads proposal requires a
/// shared memory to declare a fixed maximum up front (unlike a plain memory, which may leave it
/// unbounded) — this is the wasm32 address-space ceiling (`65536 * 64KiB` = 4 GiB), so it does not
/// otherwise constrain how far the bump-pointer heap (`memory.grow`) can grow.
pub const MAX_MEMORY_PAGES: u32 = 65536;

/// Base address (block start) of the interned string data segment; the heap begins above it.
pub const STRING_BASE: u32 = 1024;

/// `RegexFlags.IgnoreCase` / `Multiline` / `DotAll` — lockstep with `dream_abi.h` and
/// `regex_flags.dream`.
pub const DREAM_REGEX_IGNORE_CASE: i32 = 2;
pub const DREAM_REGEX_MULTILINE: i32 = 4;
pub const DREAM_REGEX_DOTALL: i32 = 8;

// -- Cross-thread allocator coordination (linear memory is `shared`, see `execution::host::shared_memory`) --
//
// The segregated free-list table occupies bytes [4, 44) for size-class heads 0..8
// (`runtime/allocator.wat`), plus large-class heads at 56+ (idx 9..12) and the huge-list head at 72.
// Two more coordination words are reserved at 44/48 — deliberately *not* overlapping freelist
// heads — both well below `STRING_BASE` (1024) so they can never collide with static data:
//
// - `ALLOC_LOCK_ADDR`: a spinlock (0 = free, 1 = held) serializing every `$malloc`/`$free` body. The
//   owner instance and every `WebWorker` instance of the same module import the *same*
//   shared linear memory, so without this lock two threads racing the free-list/bump-pointer
//   logic concurrently would corrupt the heap (lost updates, double-allocated blocks).
// - `HEAP_PTR_ADDR`: the bump-pointer high-water mark, moved out of a WASM global (which is
//   per-*instance*, not per-memory) into shared memory so every thread bumps the same pointer.
//   Initialized exactly once across however many instances share this memory, via an atomic
//   compare-exchange from 0 in `$__runtime_init` (see `emit/module.rs`) — whichever instance runs
//   first wins the exchange, every later instantiation's exchange is a no-op.
pub const ALLOC_LOCK_ADDR: u32 = 44;
pub const HEAP_PTR_ADDR: u32 = 48;

/// A monotonically increasing counter (`i32.atomic.rmw.add`) handing out a small, dense, unique id
/// to each thread that ever calls `$__thread_id` (see `runtime/sync.wat`) — the owner instance and
/// every `WebWorker` instance draw from this one shared word, so ids never collide across threads.
/// Each thread caches its own id in the ordinary (per-*instance*) WASM global `$__tid` after the
/// first call, so every later call is a single `global.get`, not a repeat atomic RMW. Backs the
/// owner-thread-id half of the reentrant lock word (`@shared class`'s embedded lock, `lock (obj)
/// { ... }`, and `Lock`) — see `HEADER_LOCK_WORD_SIZE` below for the lock word's own layout.
pub const THREAD_ID_COUNTER_ADDR: u32 = 52;

/// Shared async run-queue / timer list heads. WASM globals are per-instance; these live in
/// shared linear memory so workers and the owner see the same scheduler lists.
pub const ASYNC_RQ_HEAD_ADDR: u32 = 76;
pub const ASYNC_RQ_TAIL_ADDR: u32 = 80;
pub const ASYNC_TIMER_HEAD_ADDR: u32 = 84;
pub const ASYNC_VCLOCK_ADDR: u32 = 88;

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

/// Pointer width / alignment for a backend, plus the layouts that depend on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TargetAbi {
    pub ptr_size: u32,
    pub ptr_align: u32,
    pub heap_header_size: u32,
    pub future: FutureLayout,
}

impl TargetAbi {
    pub const WASM32: Self = Self {
        ptr_size: 4,
        ptr_align: 4,
        heap_header_size: HEAP_HEADER_SIZE,
        future: FutureLayout::WASM32,
    };

    pub fn native() -> Self {
        Self {
            ptr_size: std::mem::size_of::<usize>() as u32,
            ptr_align: std::mem::align_of::<usize>() as u32,
            heap_header_size: NATIVE_HEAP_HEADER_SIZE,
            future: FutureLayout::native(),
        }
    }
}

pub const FUTURE_KIND_TASK: i32 = 0;
pub const FUTURE_KIND_HOST: i32 = 1;
pub const FUTURE_KIND_ALL: i32 = 2;
pub const FUTURE_KIND_ANY: i32 = 3;
pub const FUTURE_STATUS_PENDING: i32 = 0;
pub const FUTURE_STATUS_READY: i32 = 1;
pub const FUTURE_STATUS_CANCELLED: i32 = 2;
pub const HOST_POLL_INDEX: i32 = -1;

/// Byte offsets of the `Future` frame header. WASM uses packed i32 fields ([`FutureLayout::WASM32`]);
/// native packs the same fields with host pointer size so `F_WIDE` never aliases `F_REMAINING`.
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
    /// Native combinator element size. `0` on wasm32 (unused; combinators live in WAT).
    pub esize: u32,
    pub wide: u32,
    /// Start of saved locals. Host-allocated futures are exactly this many bytes.
    pub slots: u32,
}

impl FutureLayout {
    /// Historical wasm32 packing: every header field is an `i32`, `F_WIDE` is 8 bytes at 56,
    /// locals start at 64. Kept as a `const` so WAT/JS/host stay byte-stable.
    pub const WASM32: Self = Self::compute(4, 4, false);

    pub fn native() -> Self {
        Self::compute(
            std::mem::size_of::<usize>() as u32,
            std::mem::align_of::<usize>() as u32,
            true,
        )
    }

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

    pub fn substitute_wat(&self, src: &str) -> String {
        src.replace("{F_RESULTS}", &self.results.to_string())
            .replace("{F_RESULT}", &self.result.to_string())
            .replace("{F_REMAINING}", &self.remaining.to_string())
            .replace("{F_AWAITING}", &self.awaiting.to_string())
            .replace("{F_CHILDREN}", &self.children.to_string())
            .replace("{F_STATUS}", &self.status.to_string())
            .replace("{F_QUEUED}", &self.queued.to_string())
            .replace("{F_WAKER}", &self.waker.to_string())
            .replace("{F_STATE}", &self.state.to_string())
            .replace("{F_SLOTS}", &self.slots.to_string())
            .replace("{F_WIDE}", &self.wide.to_string())
            .replace("{F_POLL}", &self.poll.to_string())
            .replace("{F_KIND}", &self.kind.to_string())
            .replace("{F_NEXT}", &self.next.to_string())
            .replace("{F_COUNT}", &self.count.to_string())
            .replace("{F_DUE}", &self.due.to_string())
            .replace("{F_ESIZE}", &self.esize.to_string())
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

/// Address of the refcount word given a data pointer.
pub fn rc_addr(data_ptr: u32) -> u32 {
    data_ptr.wrapping_sub(RC_FROM_DATA)
}

/// Address of the first array element given an array data pointer (`[len][payload…]`).
pub fn elem_base(array_ptr: u32) -> u32 {
    array_ptr.wrapping_add(LEN_PREFIX_SIZE)
}

// -- Runtime export / import symbol names --------------------------------------------------------
//
// The names below form the contract between the emitted module and every host (`execution/host`,
// `runtime/dream.js`, native C) plus the passes that special-case the entry point. Keeping
// them here means a rename is a single edit.

/// The program entry point exported to, and invoked by, the host.
pub const ENTRY_FN: &str = "main";
/// Native C symbol that wraps [`ENTRY_FN`]; wasm32 exports [`ENTRY_FN`] as the function name.
pub const GUEST_ENTRY_FN: &str = "dream_guest_entry";

/// Host import module for the fixed `print_*` builtins.
pub const ENV_MODULE: &str = "env";

pub const PRINT_STRING: &str = "print_string";
pub const PRINT_INT: &str = "print_int";
pub const PRINT_FLOAT: &str = "print_float";
pub const PRINT_DOUBLE: &str = "print_double";
pub const PRINT_CHAR: &str = "print_char";

/// `(import name, wasm value kind)` for `env` print builtins. `I32`/`F32`/`F64` as type tags.
pub const ENV_PRINT_IMPORTS: &[(&str, PrintVal)] = &[
    (PRINT_STRING, PrintVal::I32),
    (PRINT_INT, PrintVal::I32),
    (PRINT_FLOAT, PrintVal::F32),
    (PRINT_DOUBLE, PrintVal::F64),
    (PRINT_CHAR, PrintVal::I32),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrintVal {
    I32,
    F32,
    F64,
}

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
/// Per-instance startup (function table, heap bump, `__dream_init`). WAT uses `(start)`; C wasm32
/// exports this so `load()` and worker instances can run it without calling `main`.
pub const EXPORT_RUNTIME_INIT: &str = "__runtime_init";

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

/// C-backend wasm32 export mapping a `dream_ft[]` dispatch index to the function-pointer value.
/// Clang assigns `__indirect_function_table` slots independently of `dream_ft[]` order, but on
/// wasm32 a function pointer *is* its table index — so the JS host translates through this before
/// `table.get` when wrapping a FUNC-slot callback (the WAT backend needs no such export).
pub const EXPORT_FT_GET: &str = "dream_ft_get";

#[cfg(test)]
mod abi_h_lockstep {
    use super::*;

    fn header_define(src: &str, name: &str) -> i64 {
        for line in src.lines() {
            let line = line.trim();
            let prefix = format!("#define {name} ");
            if let Some(rest) = line.strip_prefix(&prefix) {
                let rest = rest.trim();
                if rest.starts_with('(') {
                    // SHADOW_STACK_SIZE (16 * WASM_PAGE_SIZE) — skip compound forms.
                    continue;
                }
                return rest
                    .parse()
                    .unwrap_or_else(|_| panic!("bad #define {}: {}", name, rest));
            }
        }
        panic!("missing #define {} in dream_abi.h", name);
    }

    #[test]
    fn dream_abi_h_matches_abi_rs() {
        let h = include_str!("runtime/c/include/dream_abi.h");
        assert_eq!(header_define(h, "TAG_INT"), TAG_INT as i64);
        assert_eq!(header_define(h, "TAG_FLOAT"), TAG_FLOAT as i64);
        assert_eq!(header_define(h, "TAG_DOUBLE"), TAG_DOUBLE as i64);
        assert_eq!(header_define(h, "TAG_BOOL"), TAG_BOOL as i64);
        assert_eq!(header_define(h, "TAG_STRING"), TAG_STRING as i64);
        assert_eq!(header_define(h, "TAG_ARRAY"), TAG_ARRAY as i64);
        assert_eq!(header_define(h, "TAG_CHAR"), TAG_CHAR as i64);
        assert_eq!(header_define(h, "TAG_LONG"), TAG_LONG as i64);
        assert_eq!(header_define(h, "TAG_UINT"), TAG_UINT as i64);
        assert_eq!(header_define(h, "TAG_ULONG"), TAG_ULONG as i64);
        assert_eq!(header_define(h, "TAG_BYTE"), TAG_BYTE as i64);
        assert_eq!(header_define(h, "TAG_STRUCT_BASE"), TAG_STRUCT_BASE as i64);
        assert_eq!(
            header_define(h, "HEAP_HEADER_SIZE"),
            HEAP_HEADER_SIZE as i64
        );
        assert_eq!(
            header_define(h, "HEADER_TAG_OFFSET"),
            HEADER_TAG_OFFSET as i64
        );
        assert_eq!(
            header_define(h, "HEADER_REFCOUNT_OFFSET"),
            HEADER_REFCOUNT_OFFSET as i64
        );
        assert_eq!(header_define(h, "LEN_PREFIX_SIZE"), LEN_PREFIX_SIZE as i64);
        assert_eq!(
            header_define(h, "STRING_HEADER_SIZE"),
            STRING_HEADER_SIZE as i64
        );
        assert_eq!(
            header_define(h, "STRING_UNITS_OFFSET"),
            STRING_UNITS_OFFSET as i64
        );
        assert_eq!(
            header_define(h, "STRING_SCALAR_LEN_OFFSET"),
            STRING_SCALAR_LEN_OFFSET as i64
        );
        assert_eq!(header_define(h, "WASM_PAGE_SIZE"), WASM_PAGE_SIZE as i64);
        assert_eq!(
            header_define(h, "INITIAL_HEAP_PAGES"),
            INITIAL_HEAP_PAGES as i64
        );
        assert_eq!(
            header_define(h, "MAX_MEMORY_PAGES"),
            MAX_MEMORY_PAGES as i64
        );
        assert_eq!(header_define(h, "STRING_BASE"), STRING_BASE as i64);
        assert_eq!(
            header_define(h, "DREAM_REGEX_IGNORE_CASE"),
            DREAM_REGEX_IGNORE_CASE as i64
        );
        assert_eq!(
            header_define(h, "DREAM_REGEX_MULTILINE"),
            DREAM_REGEX_MULTILINE as i64
        );
        assert_eq!(
            header_define(h, "DREAM_REGEX_DOTALL"),
            DREAM_REGEX_DOTALL as i64
        );
        assert_eq!(header_define(h, "ALLOC_LOCK_ADDR"), ALLOC_LOCK_ADDR as i64);
        assert_eq!(header_define(h, "HEAP_PTR_ADDR"), HEAP_PTR_ADDR as i64);
        assert_eq!(
            header_define(h, "THREAD_ID_COUNTER_ADDR"),
            THREAD_ID_COUNTER_ADDR as i64
        );
        assert_eq!(
            header_define(h, "ASYNC_RQ_HEAD_ADDR"),
            ASYNC_RQ_HEAD_ADDR as i64
        );
        assert_eq!(
            header_define(h, "ASYNC_RQ_TAIL_ADDR"),
            ASYNC_RQ_TAIL_ADDR as i64
        );
        assert_eq!(
            header_define(h, "ASYNC_TIMER_HEAD_ADDR"),
            ASYNC_TIMER_HEAD_ADDR as i64
        );
        assert_eq!(
            header_define(h, "ASYNC_VCLOCK_ADDR"),
            ASYNC_VCLOCK_ADDR as i64
        );
        assert_eq!(
            header_define(h, "HEADER_LOCK_WORD_SIZE"),
            HEADER_LOCK_WORD_SIZE as i64
        );
        assert_eq!(header_define(h, "LOCK_DEPTH_BITS"), LOCK_DEPTH_BITS as i64);
        assert_eq!(
            header_define(h, "NATIVE_HEAP_HEADER_SIZE"),
            NATIVE_HEAP_HEADER_SIZE as i64
        );
        assert_eq!(
            header_define(h, "DREAM_STR_PAD_INLINE"),
            DREAM_STR_PAD_INLINE as i64
        );
        assert_eq!(header_define(h, "DREAM_STR_SLICE"), DREAM_STR_SLICE as i64);
        assert_eq!(header_define(h, "RC_FROM_DATA"), RC_FROM_DATA as i64);
        assert_eq!(header_define(h, "TAG_FROM_DATA"), TAG_FROM_DATA as i64);
        let w = FutureLayout::WASM32;
        assert_eq!(w.state, 0);
        assert_eq!(w.status, 4);
        assert_eq!(w.result, 8);
        assert_eq!(w.poll, 12);
        assert_eq!(w.waker, 16);
        assert_eq!(w.awaiting, 20);
        assert_eq!(w.kind, 24);
        assert_eq!(w.children, 28);
        assert_eq!(w.count, 32);
        assert_eq!(w.remaining, 36);
        assert_eq!(w.results, 40);
        assert_eq!(w.next, 44);
        assert_eq!(w.queued, 48);
        assert_eq!(w.due, 52);
        assert_eq!(w.wide, 56);
        assert_eq!(w.slots, 64);
        assert_eq!(w.esize, 0);
        assert_ne!(w.wide, w.remaining);
        for (name, want) in [
            ("F_STATE_WASM", w.state),
            ("F_STATUS_WASM", w.status),
            ("F_RESULT_WASM", w.result),
            ("F_POLL_WASM", w.poll),
            ("F_WAKER_WASM", w.waker),
            ("F_AWAITING_WASM", w.awaiting),
            ("F_KIND_WASM", w.kind),
            ("F_CHILDREN_WASM", w.children),
            ("F_COUNT_WASM", w.count),
            ("F_REMAINING_WASM", w.remaining),
            ("F_RESULTS_WASM", w.results),
            ("F_NEXT_WASM", w.next),
            ("F_QUEUED_WASM", w.queued),
            ("F_DUE_WASM", w.due),
            ("F_WIDE_WASM", w.wide),
            ("F_SLOTS_WASM", w.slots),
        ] {
            assert_eq!(header_define(h, name), want as i64, "{name}");
        }
        let n = FutureLayout::native();
        assert_ne!(n.wide, n.remaining);
        assert!(n.wide + 8 <= n.slots);
        assert!(n.esize + 4 <= n.wide || n.wide + 8 <= n.esize);
        assert_eq!(std::mem::size_of::<usize>(), 8);
        for (name, want) in [
            ("F_STATE_NATIVE", n.state),
            ("F_STATUS_NATIVE", n.status),
            ("F_RESULT_NATIVE", n.result),
            ("F_POLL_NATIVE", n.poll),
            ("F_WAKER_NATIVE", n.waker),
            ("F_AWAITING_NATIVE", n.awaiting),
            ("F_KIND_NATIVE", n.kind),
            ("F_CHILDREN_NATIVE", n.children),
            ("F_COUNT_NATIVE", n.count),
            ("F_REMAINING_NATIVE", n.remaining),
            ("F_RESULTS_NATIVE", n.results),
            ("F_NEXT_NATIVE", n.next),
            ("F_QUEUED_NATIVE", n.queued),
            ("F_DUE_NATIVE", n.due),
            ("F_ESIZE_NATIVE", n.esize),
            ("F_WIDE_NATIVE", n.wide),
            ("F_SLOTS_NATIVE", n.slots),
        ] {
            assert_eq!(header_define(h, name), want as i64, "{name}");
        }
        assert_eq!(
            SHADOW_STACK_SIZE,
            16 * WASM_PAGE_SIZE,
            "keep dream_abi.h SHADOW_STACK_SIZE in sync"
        );
    }

    #[test]
    fn core_js_tags_match_abi_rs() {
        let js = include_str!("../../../runtime/src/core.js");
        let marshal = include_str!("../../../runtime/src/marshal.js");
        fn js_num(src: &str, key: &str) -> i64 {
            let pat = format!("{key}:");
            for line in src.lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix(&pat) {
                    let n = rest.trim().trim_end_matches(',');
                    return n.parse().unwrap_or_else(|_| panic!("bad {}: {}", key, n));
                }
            }
            panic!("missing {} in JS", key);
        }
        fn js_assign(src: &str, name: &str) -> i64 {
            let pat = format!("export const {name} = ");
            for line in src.lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix(&pat) {
                    let n = rest.split(['/', ';']).next().unwrap().trim();
                    return n.parse().unwrap_or_else(|_| panic!("bad {}: {}", name, n));
                }
            }
            panic!("missing {} in JS", name);
        }
        assert_eq!(js_num(js, "INT"), TAG_INT as i64);
        assert_eq!(js_num(js, "FLOAT"), TAG_FLOAT as i64);
        assert_eq!(js_num(js, "DOUBLE"), TAG_DOUBLE as i64);
        assert_eq!(js_num(js, "BOOL"), TAG_BOOL as i64);
        assert_eq!(js_num(js, "STRING"), TAG_STRING as i64);
        assert_eq!(js_num(js, "ARRAY"), TAG_ARRAY as i64);
        assert_eq!(js_num(js, "CHAR"), TAG_CHAR as i64);
        assert_eq!(js_num(js, "LONG"), TAG_LONG as i64);
        assert_eq!(js_num(js, "UINT"), TAG_UINT as i64);
        assert_eq!(js_num(js, "ULONG"), TAG_ULONG as i64);
        assert_eq!(js_num(js, "BYTE"), TAG_BYTE as i64);
        assert_eq!(js_num(js, "STRUCT_BASE"), TAG_STRUCT_BASE as i64);
        assert_eq!(js_assign(js, "HEAP_HEADER_SIZE"), HEAP_HEADER_SIZE as i64);
        assert_eq!(
            js_assign(marshal, "FUTURE_SLOTS_SIZE"),
            FutureLayout::WASM32.slots as i64
        );
    }
}

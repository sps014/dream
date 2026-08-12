use super::*;

/// The allocator + GC + string runtime. When `debug` is on (the default compiler mode), `$gc_alloc`
/// bumps `$live_objects`/`$total_allocations` and `$free` decrements `$live_objects` (backing the
/// `Debug.*` probes); under `--release` the placeholders expand to nothing so the hot allocation
/// path carries no extra instructions.
///
/// When `needs_threads` is false (no `WebWorker` / worker-pool host imports in the module), the
/// allocator spinlock around `$malloc`/`$free`/`$gc_collect_*` is also elided: a single-threaded
/// instance never races on the free lists, so the atomic acquire/release is pure overhead.
pub(super) fn runtime_prelude(debug: bool, needs_threads: bool) -> String {
    let (malloc_count, free_count) = if debug {
        (
            "global.get $live_objects\n    i32.const 1\n    i32.add\n    global.set $live_objects\n    \
             global.get $total_allocations\n    i32.const 1\n    i32.add\n    global.set $total_allocations",
            "global.get $live_objects\n    i32.const 1\n    i32.sub\n    global.set $live_objects",
        )
    } else {
        ("", "")
    };
    let (lock_acquire, lock_release) = if needs_threads {
        ("call $__alloc_lock_acquire", "call $__alloc_lock_release")
    } else {
        ("", "")
    };
    let mut out = RUNTIME_ALLOCATOR
        .replace(";;@DEBUG_ALLOC_COUNT@", malloc_count)
        .replace(";;@DEBUG_FREE_COUNT@", free_count)
        .replace(";;@ALLOC_LOCK_ACQUIRE@", lock_acquire)
        .replace(";;@ALLOC_LOCK_RELEASE@", lock_release)
        .replace(
            "{ALLOC_LOCK_ADDR}",
            &crate::abi::ALLOC_LOCK_ADDR.to_string(),
        )
        .replace("{HEAP_PTR_ADDR}", &crate::abi::HEAP_PTR_ADDR.to_string())
        .replace(
            "{GC_META_FREE}",
            &crate::abi::GC_META_FREE.to_string(),
        );
    out.push('\n');
    out.push_str(&substitute_gc_runtime(
        lock_acquire,
        lock_release,
        malloc_count,
        free_count,
    ));
    out.push('\n');
    // The string runtime tags freshly allocated string blocks with the heap `TAG_STRING`. `$char_at`
    // itself no longer bounds-checks: callers emit a located check inline before calling it (see
    // `Emitter::emit_char_at`), so a string-index panic gets a precise file:line rather than the one
    // bare, unlocated message a truly shared runtime helper would be stuck with.
    out.push_str(
        &RUNTIME_STRINGS
            .replace("{TAG_STRING}", &crate::abi::TAG_STRING.to_string())
            .replace("{HEAP_PTR_ADDR}", &crate::abi::HEAP_PTR_ADDR.to_string()),
    );
    out
}

fn substitute_gc_runtime(
    lock_acquire: &str,
    lock_release: &str,
    malloc_count: &str,
    free_count: &str,
) -> String {
    use crate::abi as a;
    RUNTIME_GC
        .replace(";;@ALLOC_LOCK_ACQUIRE@", lock_acquire)
        .replace(";;@ALLOC_LOCK_RELEASE@", lock_release)
        .replace(";;@DEBUG_ALLOC_COUNT@", malloc_count)
        .replace(";;@DEBUG_FREE_COUNT@", free_count)
        .replace("{ALLOC_LOCK_ADDR}", &a::ALLOC_LOCK_ADDR.to_string())
        .replace("{HEAP_PTR_ADDR}", &a::HEAP_PTR_ADDR.to_string())
        .replace("{GC_META_GEN_MASK}", &a::GC_META_GEN_MASK.to_string())
        .replace("{GC_META_MARK}", &a::GC_META_MARK.to_string())
        .replace("{GC_META_FORWARDED}", &a::GC_META_FORWARDED.to_string())
        .replace("{GC_META_FINALIZE}", &a::GC_META_FINALIZE.to_string())
        .replace("{GC_META_FINALIZED}", &a::GC_META_FINALIZED.to_string())
        .replace("{GC_META_IMMORTAL}", &a::GC_META_IMMORTAL.to_string())
        .replace("{GC_META_FREE}", &a::GC_META_FREE.to_string())
        .replace("{GC_GEN0}", &a::GC_GEN0.to_string())
        .replace("{GC_GEN1}", &a::GC_GEN1.to_string())
        .replace("{GC_GEN2}", &a::GC_GEN2.to_string())
        .replace("{GC_GEN_LOH}", &a::GC_GEN_LOH.to_string())
        .replace("{LOH_THRESHOLD}", &a::LOH_THRESHOLD.to_string())
        .replace("{NURSERY_SIZE}", &a::NURSERY_SIZE.to_string())
        .replace("{NURSERY_BUMP_ADDR}", &a::NURSERY_BUMP_ADDR.to_string())
        .replace("{NURSERY_START_ADDR}", &a::NURSERY_START_ADDR.to_string())
        .replace("{NURSERY_END_ADDR}", &a::NURSERY_END_ADDR.to_string())
        .replace("{OLD_START_ADDR}", &a::OLD_START_ADDR.to_string())
        .replace("{GC_REQUEST_ADDR}", &a::GC_REQUEST_ADDR.to_string())
        .replace(
            "{GC_SAFEPOINT_EXPECT_ADDR}",
            &a::GC_SAFEPOINT_EXPECT_ADDR.to_string(),
        )
        .replace("{GC_SAFEPOINT_ACK_ADDR}", &a::GC_SAFEPOINT_ACK_ADDR.to_string())
        .replace("{GC_COLLECT_KIND_ADDR}", &a::GC_COLLECT_KIND_ADDR.to_string())
        .replace("{GC_ROOT_COUNT_ADDR}", &a::GC_ROOT_COUNT_ADDR.to_string())
        .replace(
            "{GC_ROOT_TABLE_PTR_ADDR}",
            &a::GC_ROOT_TABLE_PTR_ADDR.to_string(),
        )
        .replace("{GC_ROOT_TABLE_CAP}", &a::GC_ROOT_TABLE_CAP.to_string())
        .replace("{GC_REMSET_COUNT_ADDR}", &a::GC_REMSET_COUNT_ADDR.to_string())
        .replace(
            "{GC_REMSET_TABLE_PTR_ADDR}",
            &a::GC_REMSET_TABLE_PTR_ADDR.to_string(),
        )
        .replace("{GC_REMEMBERED_CAP}", &a::GC_REMEMBERED_CAP.to_string())
        .replace(
            "{GC_REMSET_OVERFLOW_ADDR}",
            &a::GC_REMSET_OVERFLOW_ADDR.to_string(),
        )
        .replace(
            "{GC_FINALIZER_HEAD_ADDR}",
            &a::GC_FINALIZER_HEAD_ADDR.to_string(),
        )
        .replace("{GC_OLD_BYTES_ADDR}", &a::GC_OLD_BYTES_ADDR.to_string())
        .replace("{GC_GEN1_THRESHOLD}", &a::GC_GEN1_THRESHOLD.to_string())
        .replace(
            "{GC_MARK_STACK_PTR_ADDR}",
            &a::GC_MARK_STACK_PTR_ADDR.to_string(),
        )
        .replace(
            "{GC_MARK_STACK_BASE_ADDR}",
            &a::GC_MARK_STACK_BASE_ADDR.to_string(),
        )
        .replace("{GC_MARK_STACK_CAP}", &a::GC_MARK_STACK_CAP.to_string())
        .replace("{TAG_ARRAY}", &a::TAG_ARRAY.to_string())
        .replace("{TAG_FLAT_ARRAY}", &a::TAG_FLAT_ARRAY.to_string())
}

/// True when this module imports any `WebWorker` / worker-pool host function. Only those programs
/// share linear memory across instances, so only they need the allocator spinlock.
pub(super) fn module_needs_threads(mir: &crate::Mir) -> bool {
    mir.imports.iter().any(|imp| {
        imp.module == "Dream"
            && matches!(
                imp.field.as_str(),
                "workerSpawn"
                    | "workerPost"
                    | "workerRecv"
                    | "workerTerminate"
                    | "workerPoolSpawn"
                    | "workerPoolDispatch"
            )
    })
}

/// Builds the `*_to_string` runtime (object formatters + generated `$bool_to_string` + the float/
/// double formatter), resolving the `{TAG_*}`/`{minus}` placeholders and the `bool` string pointers
/// from the interned string table. Depends on the allocator + string runtime emitted before it.
pub(super) fn to_string_runtime(strings: &IndexMap<String, u32>) -> String {
    use crate::abi as tags;
    let object = RUNTIME_OBJECT
        .replace("{TAG_INT}", &tags::TAG_INT.to_string())
        .replace("{TAG_FLOAT}", &tags::TAG_FLOAT.to_string())
        .replace("{TAG_DOUBLE}", &tags::TAG_DOUBLE.to_string())
        .replace("{TAG_BOOL}", &tags::TAG_BOOL.to_string())
        .replace("{TAG_STRING}", &tags::TAG_STRING.to_string())
        .replace("{TAG_CHAR}", &tags::TAG_CHAR.to_string())
        .replace("{TAG_LONG}", &tags::TAG_LONG.to_string())
        .replace("{TAG_UINT}", &tags::TAG_UINT.to_string())
        .replace("{TAG_ULONG}", &tags::TAG_ULONG.to_string())
        .replace("{TAG_BYTE}", &tags::TAG_BYTE.to_string());
    let t = strings["true"];
    let f = strings["false"];
    let minus = strings["-"];
    let bool_to_string = format!(
        "(func $bool_to_string (param $v i32) (result i32)\n  local.get $v\n  (if (result i32)\n    (then i32.const {t})\n    (else i32.const {f})))\n"
    );
    let format = RUNTIME_FORMAT
        .replace("{minus}", &minus.to_string())
        .replace("{TAG_STRING}", &tags::TAG_STRING.to_string());
    format!("{object}\n{bool_to_string}\n{format}\n")
}

/// The heap starts (8-byte aligned) above the interned string segment, never below the string base.
/// Each interned string's mapped address points at its byte_len word; its block extends
/// [`STRING_HEADER_SIZE`] + utf8 length beyond that (`[byte_len][scalar_len][utf8]`, no NUL).
pub(super) fn heap_base(strings: &IndexMap<String, u32>) -> u32 {
    let end = strings
        .iter()
        .map(|(s, addr)| addr + crate::abi::STRING_HEADER_SIZE + s.len() as u32)
        .max()
        .unwrap_or(STRING_BASE);
    (end.max(STRING_BASE) + 7) & !7
}

/// Reserves permanent static storage for every value-struct/value-union module global, starting at
/// `start` (typically past the interned-string region). Returns `(global_id → address, end)` where
/// `end` is 8-byte-aligned past the last slot (suitable as the itable base). Memory is assumed
/// zero-filled; `$__dream_init` constructs into these addresses.
pub(super) fn value_global_addrs(
    mir: &crate::Mir,
    interner: &TypeInterner,
    start: u32,
) -> (HashMap<u32, u32>, u32) {
    let mut addr = start;
    let mut map = HashMap::new();
    for g in &mir.globals {
        if !interner.is_value_type(g.ty) {
            continue;
        }
        let (size, align) = interner.value_layout(g.ty).unwrap_or((4, 4));
        let align = align.max(1);
        addr = (addr + align - 1) & !(align - 1);
        map.insert(g.id.0, addr);
        addr = addr.saturating_add(size);
    }
    let end = (addr + 7) & !7;
    (map, end)
}

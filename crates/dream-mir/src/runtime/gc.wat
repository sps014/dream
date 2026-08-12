;; --- Tiered GC (Gen0 nursery + Gen1/Gen2 + LOH) -------------------------------------------------
;;
;; See docs/compiler/12-tiered-gc.md and abi.rs. Heap header: [size][tag][gc_meta], data at +12.
;; Gen0 is a fixed bump nursery at heap base; survivors evacuate into old space (Gen1).
;; Older gens and LOH use mark-sweep into the segregated freelist. Write barriers record
;; older→younger slots in the remembered set.

;; Trace mode: 0 = Gen0 update (forward nursery slots), 1 = mark children.
(global $gc_trace_mode (mut i32) (i32.const 0))
;; Immutable-after-init copies of the shared-memory GC bounds/table bases. Linear memory stays
;; the source of truth for workers; `$__gc_cache_bounds` hydrates these per instance so hot
;; mutator helpers (`$write_barrier`, `$gc_root_*`, `$malloc`) avoid a memory load per call.
(global $__gc_nursery_start (mut i32) (i32.const 0))
(global $__gc_nursery_end (mut i32) (i32.const 0))
(global $__gc_old_start (mut i32) (i32.const 0))
(global $__gc_root_table (mut i32) (i32.const 0))
(global $__gc_remset_table (mut i32) (i32.const 0))

(func $__gc_cache_bounds
    i32.const {NURSERY_START_ADDR}
    i32.load
    global.set $__gc_nursery_start
    i32.const {NURSERY_END_ADDR}
    i32.load
    global.set $__gc_nursery_end
    i32.const {OLD_START_ADDR}
    i32.load
    global.set $__gc_old_start
    i32.const {GC_ROOT_TABLE_PTR_ADDR}
    i32.load
    global.set $__gc_root_table
    i32.const {GC_REMSET_TABLE_PTR_ADDR}
    i32.load
    global.set $__gc_remset_table
)

(func $__gc_meta_gen (param $meta i32) (result i32)
    local.get $meta
    i32.const {GC_META_GEN_MASK}
    i32.and
)

;; Data-pointer nursery test: managed data ptrs sit at block+12, so they still fall in
;; `[nursery_start, nursery_end)` for every well-formed nursery object (header is 12 bytes).
(func $__gc_in_nursery (param $ptr i32) (result i32)
    local.get $ptr
    i32.eqz
    if (result i32)
        i32.const 0
    else
        local.get $ptr
        global.get $__gc_nursery_start
        i32.ge_u
        if (result i32)
            local.get $ptr
            global.get $__gc_nursery_end
            i32.lt_u
        else
            i32.const 0
        end
    end
)

(func $gc_root_push (param $ptr i32) (result i32)
    (local $idx i32)
    i32.const {GC_ROOT_COUNT_ADDR}
    i32.load
    local.set $idx
    local.get $idx
    i32.const {GC_ROOT_TABLE_CAP}
    i32.ge_u
    (if (then unreachable))
    global.get $__gc_root_table
    local.get $idx
    i32.const 2
    i32.shl
    i32.add
    local.get $ptr
    i32.store
    i32.const {GC_ROOT_COUNT_ADDR}
    local.get $idx
    i32.const 1
    i32.add
    i32.store
    local.get $idx
)

(func $gc_root_set (param $idx i32) (param $ptr i32)
    global.get $__gc_root_table
    local.get $idx
    i32.const 2
    i32.shl
    i32.add
    local.get $ptr
    i32.store
)

;; Value-root reload (mutator locals / `$__obj` / module globals). Slot roots are updated in
;; place during collection and are never read through this helper.
(func $gc_root_get (param $idx i32) (result i32)
    global.get $__gc_root_table
    local.get $idx
    i32.const 2
    i32.shl
    i32.add
    i32.load
)

;; Register a memory slot (shadow-stack / heap field) as a root. The collector loads and
;; updates the pointer at `addr` in place (low bit tags the root-table entry).
(func $gc_root_push_slot (param $addr i32) (result i32)
    (local $idx i32)
    i32.const {GC_ROOT_COUNT_ADDR}
    i32.load
    local.set $idx
    local.get $idx
    i32.const {GC_ROOT_TABLE_CAP}
    i32.ge_u
    (if (then unreachable))
    global.get $__gc_root_table
    local.get $idx
    i32.const 2
    i32.shl
    i32.add
    local.get $addr
    i32.const 1
    i32.or
    i32.store
    i32.const {GC_ROOT_COUNT_ADDR}
    local.get $idx
    i32.const 1
    i32.add
    i32.store
    local.get $idx
)

(func $gc_root_pop (param $idx i32)
    ;; Shrink the root table to `idx` (exclusive end). Callers push contiguous frames.
    i32.const {GC_ROOT_COUNT_ADDR}
    local.get $idx
    i32.store
)

(func $write_barrier (param $slot i32) (param $new_val i32)
    (local $count i32)
    local.get $new_val
    i32.eqz
    br_if 0
    ;; Nursery destination: young→young is invisible to ephemeral collections.
    local.get $slot
    global.get $__gc_old_start
    i32.lt_u
    br_if 0
    local.get $new_val
    global.get $__gc_nursery_start
    i32.lt_u
    br_if 0
    local.get $new_val
    global.get $__gc_nursery_end
    i32.ge_u
    br_if 0
    i32.const {GC_REMSET_COUNT_ADDR}
    i32.load
    local.set $count
    local.get $count
    i32.const {GC_REMEMBERED_CAP}
    i32.ge_u
    if
        ;; Remset full: keep every existing entry. Flag overflow so Gen0 walks all live
        ;; old/LOH objects for young pointers. Never wipe the count (that dropped edges and
        ;; left dangling nursery refs in Maps after nursery reset). The next nursery-full
        ;; collect drains via `$__gc_collect_locked` (overflow scan covers unrecorded edges).
        i32.const {GC_REMSET_OVERFLOW_ADDR}
        i32.const 1
        i32.store
        i32.const {GC_REQUEST_ADDR}
        i32.const 1
        i32.store
        return
    end
    global.get $__gc_remset_table
    local.get $count
    i32.const 2
    i32.shl
    i32.add
    local.get $slot
    i32.store
    i32.const {GC_REMSET_COUNT_ADDR}
    local.get $count
    i32.const 1
    i32.add
    i32.store
)

;; Forward a nursery pointer: if already forwarded return the new data ptr; else copy into old
;; space as Gen1 and return the new data ptr. Non-nursery / null returned unchanged.
(func $gc_forward (param $ptr i32) (result i32)
    (local $block i32)
    (local $meta i32)
    (local $size i32)
    (local $new_block i32)
    (local $new_ptr i32)
    local.get $ptr
    i32.eqz
    if (result i32)
        i32.const 0
    else
        local.get $ptr
        call $__gc_in_nursery
        i32.eqz
        if (result i32)
            local.get $ptr
        else
            local.get $ptr
            i32.const 12
            i32.sub
            local.set $block
            local.get $block
            i32.const 8
            i32.add
            i32.load
            local.set $meta
            local.get $meta
            i32.const {GC_META_FORWARDED}
            i32.and
            if (result i32)
                local.get $block
                i32.const 4
                i32.add
                i32.load
            else
                local.get $block
                i32.load
                local.set $size
                local.get $meta
                i32.const {GC_META_IMMORTAL}
                i32.and
                if (result i32)
                    local.get $ptr
                else
                    local.get $size
                    i32.const 12
                    i32.sub
                    local.get $block
                    i32.const 4
                    i32.add
                    i32.load
                    local.get $meta
                    i32.const {GC_META_GEN_MASK}
                    i32.const -1
                    i32.xor
                    i32.and
                    i32.const {GC_GEN1}
                    i32.or
                    call $__gc_alloc_old
                    local.set $new_ptr
                    local.get $new_ptr
                    i32.const 12
                    i32.sub
                    local.set $new_block
                    local.get $new_block
                    local.get $block
                    local.get $size
                    memory.copy
                    local.get $new_block
                    i32.const 8
                    i32.add
                    local.get $meta
                    i32.const {GC_META_GEN_MASK}
                    i32.const -1
                    i32.xor
                    i32.and
                    i32.const {GC_GEN1}
                    i32.or
                    i32.const {GC_META_MARK}
                    i32.or
                    i32.store
                    ;; Keep size at +0 so nursery walks stay valid; stow forward ptr in tag word.
                    local.get $block
                    i32.const 4
                    i32.add
                    local.get $new_ptr
                    i32.store
                    local.get $block
                    i32.const 8
                    i32.add
                    local.get $meta
                    i32.const {GC_META_FORWARDED}
                    i32.or
                    i32.store
                    local.get $new_ptr
                    call $gc_trace_evacuated
                    local.get $new_ptr
                end
            end
        end
    end
)

;; Trace a just-evacuated object in Gen0 update mode: structs via `$gc_trace_object`, arrays via
;; a length-prefixed slot walk, funcboxes via `$gc_trace_funcbox`.
(func $gc_trace_evacuated (param $ptr i32)
    (local $tag i32)
    (local $slot i32)
    (local $len i32)
    (local $i i32)
    local.get $ptr
    i32.eqz
    br_if 0
    local.get $ptr
    call $object_tag
    local.set $tag
    local.get $tag
    i32.const {TAG_ARRAY}
    i32.eq
    if
        ;; TAG_ARRAY only (ref elems). TAG_FLAT_ARRAY is blittable — skip payload.
        local.get $ptr
        i32.load
        local.set $len
        i32.const 0
        local.set $i
        (loop $elems
            local.get $i
            local.get $len
            i32.ge_s
            br_if 1
            local.get $ptr
            i32.const 4
            i32.add
            local.get $i
            i32.const 2
            i32.shl
            i32.add
            call $gc_update_slot
            local.get $i
            i32.const 1
            i32.add
            local.set $i
            br $elems
        )
        return
    end
    local.get $tag
    i32.const {TAG_FLAT_ARRAY}
    i32.eq
    if
        return
    end
    local.get $tag
    i32.eqz
    if
        ;; funcbox tag 0
        local.get $ptr
        call $gc_trace_funcbox
        return
    end
    local.get $ptr
    call $gc_trace_object
)

;; Trace/update one heap reference slot during Gen0 collection.
(func $gc_update_slot (param $slot i32)
    (local $v i32)
    local.get $slot
    i32.load
    local.set $v
    local.get $v
    call $__gc_in_nursery
    i32.eqz
    br_if 0
    local.get $slot
    local.get $v
    call $gc_forward
    i32.store
)

(func $gc_scan_roots
    (local $i i32)
    (local $n i32)
    (local $base i32)
    (local $slot i32)
    (local $entry i32)
    i32.const {GC_ROOT_COUNT_ADDR}
    i32.load
    local.set $n
    global.get $__gc_root_table
    local.set $base
    i32.const 0
    local.set $i
    (loop $roots
        local.get $i
        local.get $n
        i32.ge_u
        br_if 1
        local.get $base
        local.get $i
        i32.const 2
        i32.shl
        i32.add
        local.set $slot
        local.get $slot
        i32.load
        local.set $entry
        local.get $entry
        i32.const 1
        i32.and
        if
            ;; memory slot root: update the pointer at `addr` in place
            local.get $entry
            i32.const -2
            i32.and
            call $gc_update_slot
        else
            ;; value root: forward and write back into the table
            local.get $slot
            local.get $entry
            call $gc_forward
            i32.store
        end
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br $roots
    )
)

(func $gc_scan_remset
    (local $i i32)
    (local $n i32)
    (local $base i32)
    (local $slot i32)
    i32.const {GC_REMSET_COUNT_ADDR}
    i32.load
    local.set $n
    global.get $__gc_remset_table
    local.set $base
    i32.const 0
    local.set $i
    (loop $rs
        local.get $i
        local.get $n
        i32.ge_u
        br_if 1
        local.get $base
        local.get $i
        i32.const 2
        i32.shl
        i32.add
        i32.load
        local.set $slot
        local.get $slot
        call $gc_update_slot
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br $rs
    )
    i32.const {GC_REMSET_COUNT_ADDR}
    i32.const 0
    i32.store
)

;; After evacuating via roots/remset, walk old/LOH for remaining young refs.
;; Always updates MARKED survivors (just evacuated). When the remset overflowed, also
;; walks every live (non-free) old block — remset alone is incomplete.
(func $gc_scan_old_for_young
    (local $p i32)
    (local $end i32)
    (local $block i32)
    (local $size i32)
    (local $meta i32)
    (local $overflow i32)
    i32.const {GC_REMSET_OVERFLOW_ADDR}
    i32.load
    local.set $overflow
    i32.const {OLD_START_ADDR}
    i32.load
    local.set $p
    i32.const {HEAP_PTR_ADDR}
    i32.load
    local.set $end
    (loop $walk
        local.get $p
        local.get $end
        i32.ge_u
        br_if 1
        local.get $p
        local.set $block
        local.get $block
        i32.load
        local.set $size
        local.get $size
        i32.eqz
        if
            local.get $p
            i32.const 16
            i32.add
            local.set $p
            br $walk
        end
        local.get $block
        i32.const 8
        i32.add
        i32.load
        local.set $meta
        local.get $meta
        i32.const {GC_META_FREE}
        i32.and
        i32.eqz
        if
            local.get $overflow
            local.get $meta
            i32.const {GC_META_MARK}
            i32.and
            i32.or
            if
                local.get $block
                i32.const 12
                i32.add
                call $gc_trace_evacuated
            end
        end
        local.get $p
        local.get $size
        i32.add
        i32.const 3
        i32.add
        i32.const -4
        i32.and
        local.set $p
        br $walk
    )
)

(func $gc_finalize_dead_nursery
    (local $p i32)
    (local $end i32)
    (local $size i32)
    (local $meta i32)
    (local $ptr i32)
    i32.const {GC_FINALIZER_HEAD_ADDR}
    i32.const 0
    i32.store
    i32.const {NURSERY_START_ADDR}
    i32.load
    local.set $p
    i32.const {NURSERY_BUMP_ADDR}
    i32.load
    local.set $end
    (loop $scan
        local.get $p
        local.get $end
        i32.ge_u
        br_if 1
        local.get $p
        i32.load
        local.set $size
        local.get $size
        i32.eqz
        if
            local.get $p
            i32.const 16
            i32.add
            local.set $p
            br $scan
        end
        local.get $p
        i32.const 8
        i32.add
        i32.load
        local.set $meta
        local.get $meta
        i32.const {GC_META_FORWARDED}
        i32.and
        i32.eqz
        if
            local.get $meta
            i32.const {GC_META_FINALIZE}
            i32.and
            if
                local.get $meta
                i32.const {GC_META_FINALIZED}
                i32.and
                i32.eqz
                if
                    local.get $p
                    i32.const 12
                    i32.add
                    call $gc_run_finalizer
                    local.get $p
                    i32.const 8
                    i32.add
                    local.get $meta
                    i32.const {GC_META_FINALIZED}
                    i32.or
                    i32.store
                end
            end
            local.get $p
            i32.const 12
            i32.add
            call $weak_clear_all
            local.get $p
            i32.const 12
            i32.add
            call $gc_drop_js_handles
            ;;@DEBUG_FREE_COUNT@
        else
            ;; Forwarded blocks: the source was counted at its original `$malloc`, and the copied
            ;; target was counted again in `$__gc_alloc_old` during evacuation. Nursery reset
            ;; abandons the source without hitting `$__free_locked`, so decrement `live_objects`
            ;; here to keep the debug counter balanced with a move (net zero across the pair).
            ;;@DEBUG_FREE_COUNT@
        end
        local.get $p
        local.get $size
        i32.add
        i32.const 3
        i32.add
        i32.const -4
        i32.and
        local.set $p
        br $scan
    )
)

(func $gc_collect_gen0
    i32.const {GC_REQUEST_ADDR}
    i32.const 1
    i32.store
    i32.const 0
    global.set $gc_trace_mode
    call $gc_scan_roots
    call $gc_scan_remset
    call $gc_scan_old_for_young
    call $gc_finalize_dead_nursery
    i32.const {NURSERY_BUMP_ADDR}
    global.get $__gc_nursery_start
    i32.store
    i32.const {GC_REMSET_COUNT_ADDR}
    i32.const 0
    i32.store
    i32.const {GC_REMSET_OVERFLOW_ADDR}
    i32.const 0
    i32.store
    i32.const {GC_REQUEST_ADDR}
    i32.const 0
    i32.store
    i32.const {GC_EPOCH_ADDR}
    i32.const {GC_EPOCH_ADDR}
    i32.load
    i32.const 1
    i32.add
    i32.store
)

;; Mark bit helpers for older gens.
(func $gc_mark_object (param $ptr i32)
    (local $block i32)
    (local $meta i32)
    (local $tag i32)
    local.get $ptr
    i32.eqz
    br_if 0
    local.get $ptr
    call $__gc_in_nursery
    if
        ;; should have been forwarded already in a full collect that did Gen0 first
        local.get $ptr
        call $gc_forward
        drop
        return
    end
    local.get $ptr
    i32.const 12
    i32.sub
    local.set $block
    local.get $block
    i32.const 8
    i32.add
    i32.load
    local.set $meta
    local.get $meta
    i32.const {GC_META_IMMORTAL}
    i32.and
    br_if 0
    local.get $meta
    i32.const {GC_META_FREE}
    i32.and
    br_if 0
    local.get $meta
    i32.const {GC_META_MARK}
    i32.and
    br_if 0
    local.get $block
    i32.const 8
    i32.add
    local.get $meta
    i32.const {GC_META_MARK}
    i32.or
    i32.store
    local.get $ptr
    call $object_tag
    local.set $tag
    ;; TAG_ARRAY / TAG_FLAT_ARRAY: ref-element arrays are visited via typed
    ;; `$gc_trace_array_t*` from parents; do not conservatively scan payloads.
    ;; Tag 0 is shared (funcbox / weak box) — only typed visitors may interpret it.
    local.get $tag
    i32.const {TAG_ARRAY}
    i32.eq
    br_if 0
    local.get $tag
    i32.const {TAG_FLAT_ARRAY}
    i32.eq
    br_if 0
    local.get $ptr
    call $gc_trace_object
)

(func $gc_clear_marks_old
    (local $p i32)
    (local $end i32)
    (local $size i32)
    (local $meta i32)
    i32.const {OLD_START_ADDR}
    i32.load
    local.set $p
    i32.const {HEAP_PTR_ADDR}
    i32.load
    local.set $end
    (loop $clr
        local.get $p
        local.get $end
        i32.ge_u
        br_if 1
        local.get $p
        i32.load
        local.set $size
        local.get $size
        i32.eqz
        if
            local.get $p
            i32.const 16
            i32.add
            local.set $p
            br $clr
        end
        local.get $p
        i32.const 8
        i32.add
        i32.load
        local.set $meta
        local.get $p
        i32.const 8
        i32.add
        local.get $meta
        i32.const {GC_META_MARK}
        i32.const -1
        i32.xor
        i32.and
        i32.store
        local.get $p
        local.get $size
        i32.add
        i32.const 3
        i32.add
        i32.const -4
        i32.and
        local.set $p
        br $clr
    )
)

(func $gc_mark_from_roots
    (local $i i32)
    (local $n i32)
    (local $base i32)
    (local $entry i32)
    i32.const {GC_ROOT_COUNT_ADDR}
    i32.load
    local.set $n
    global.get $__gc_root_table
    local.set $base
    i32.const 0
    local.set $i
    (loop $m
        local.get $i
        local.get $n
        i32.ge_u
        br_if 1
        local.get $base
        local.get $i
        i32.const 2
        i32.shl
        i32.add
        i32.load
        local.set $entry
        local.get $entry
        i32.const 1
        i32.and
        if (result i32)
            local.get $entry
            i32.const -2
            i32.and
            i32.load
        else
            local.get $entry
        end
        call $gc_mark_object
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br $m
    )
)

;; Sweep unmarked old/LOH objects onto the freelist; enqueue finalizers.
(func $gc_sweep_old
    (local $p i32)
    (local $end i32)
    (local $size i32)
    (local $meta i32)
    (local $ptr i32)
    (local $gen i32)
    i32.const {OLD_START_ADDR}
    i32.load
    local.set $p
    i32.const {HEAP_PTR_ADDR}
    i32.load
    local.set $end
    (loop $sw
        local.get $p
        local.get $end
        i32.ge_u
        br_if 1
        local.get $p
        i32.load
        local.set $size
        local.get $size
        i32.eqz
        if
            local.get $p
            i32.const 16
            i32.add
            local.set $p
            br $sw
        end
        local.get $p
        i32.const 8
        i32.add
        i32.load
        local.set $meta
        local.get $meta
        i32.const {GC_META_IMMORTAL}
        i32.and
        if
            local.get $p
            local.get $size
            i32.add
            i32.const 3
            i32.add
            i32.const -4
            i32.and
            local.set $p
            br $sw
        end
            local.get $meta
            i32.const {GC_META_MARK}
            i32.and
            i32.eqz
            if
                local.get $p
                i32.const 12
                i32.add
                local.set $ptr
                local.get $meta
                i32.const {GC_META_FINALIZE}
                i32.and
                if
                    local.get $meta
                    i32.const {GC_META_FINALIZED}
                    i32.and
                    i32.eqz
                    if
                        local.get $ptr
                        call $gc_enqueue_finalizer
                        ;; leave allocated until `$gc_run_finalizers` frees after `del`
                        local.get $p
                        local.get $size
                        i32.add
                        i32.const 3
                        i32.add
                        i32.const -4
                        i32.and
                        local.set $p
                        br $sw
                    end
                end
                local.get $ptr
                call $weak_clear_all
                local.get $ptr
                call $gc_drop_js_handles
                local.get $ptr
                call $__free_locked
            else
            ;; promote Gen1 survivors to Gen2
            local.get $meta
            call $__gc_meta_gen
            local.set $gen
            local.get $gen
            i32.const {GC_GEN1}
            i32.eq
            if
                local.get $p
                i32.const 8
                i32.add
                local.get $meta
                i32.const {GC_META_GEN_MASK}
                i32.const -1
                i32.xor
                i32.and
                i32.const {GC_GEN2}
                i32.or
                i32.const {GC_META_MARK}
                i32.const -1
                i32.xor
                i32.and
                i32.store
            else
                local.get $p
                i32.const 8
                i32.add
                local.get $meta
                i32.const {GC_META_MARK}
                i32.const -1
                i32.xor
                i32.and
                i32.store
            end
        end
        local.get $p
        local.get $size
        i32.add
        i32.const 3
        i32.add
        i32.const -4
        i32.and
        local.set $p
        br $sw
    )
)

(func $gc_enqueue_finalizer (param $ptr i32)
    ;; Side list: reuse first payload word temporarily? Prefer a linked list via a dedicated
    ;; header in a small external queue node. For v1, call `$gc_run_finalizer` immediately
    ;; after weak clear and before free — queued semantics approximated by running here when
    ;; `$gc_run_finalizers` drains. Store ptr in a simple singly-linked list using freelist-like
    ;; nodes is complex; instead push onto a dense stack in the remset scratch after remset clear.
    (local $count i32)
    (local $base i32)
    i32.const {GC_FINALIZER_HEAD_ADDR}
    i32.load
    local.set $count
    ;; Reuse remset table as a finalizer stack when count is stored at FINALIZER_HEAD as length.
    global.get $__gc_remset_table
    local.set $base
    local.get $count
    i32.const {GC_REMEMBERED_CAP}
    i32.ge_u
    (if (then unreachable))
    local.get $base
    local.get $count
    i32.const 2
    i32.shl
    i32.add
    local.get $ptr
    i32.store
    i32.const {GC_FINALIZER_HEAD_ADDR}
    local.get $count
    i32.const 1
    i32.add
    i32.store
)

(func $gc_run_finalizers
    (local $i i32)
    (local $n i32)
    (local $base i32)
    (local $ptr i32)
    i32.const {GC_FINALIZER_HEAD_ADDR}
    i32.load
    local.set $n
    global.get $__gc_remset_table
    local.set $base
    i32.const 0
    local.set $i
    (loop $fin
        local.get $i
        local.get $n
        i32.ge_u
        br_if 1
        local.get $base
        local.get $i
        i32.const 2
        i32.shl
        i32.add
        i32.load
        local.set $ptr
        local.get $ptr
        call $gc_run_finalizer
        local.get $ptr
        i32.const 4
        i32.sub
        local.get $ptr
        i32.const 4
        i32.sub
        i32.load
        i32.const {GC_META_FINALIZED}
        i32.or
        i32.store
        local.get $ptr
        call $weak_clear_all
        local.get $ptr
        call $gc_drop_js_handles
        ;; Nursery corpses are abandoned when the bump resets; only old/LOH need `$free`.
        local.get $ptr
        call $__gc_in_nursery
        i32.eqz
        if
            local.get $ptr
            call $__free_locked
        end
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br $fin
    )
    i32.const {GC_FINALIZER_HEAD_ADDR}
    i32.const 0
    i32.store
)

(func $gc_collect_old
    i32.const 1
    global.set $gc_trace_mode
    call $gc_clear_marks_old
    call $gc_mark_from_roots
    i32.const {GC_FINALIZER_HEAD_ADDR}
    i32.const 0
    i32.store
    call $gc_sweep_old
    call $gc_run_finalizers
    i32.const {GC_OLD_BYTES_ADDR}
    i32.const 0
    i32.store
    i32.const {GC_EPOCH_ADDR}
    i32.const {GC_EPOCH_ADDR}
    i32.load
    i32.const 1
    i32.add
    i32.store
)

;; kind: 0 = Gen0 only, 1 = Gen0+Gen1, 2 = full (Gen0+old+LOH). Caller must hold alloc lock.
(func $__gc_collect_locked (param $kind i32)
    i32.const {GC_COLLECT_KIND_ADDR}
    local.get $kind
    i32.store
    call $gc_collect_gen0
    local.get $kind
    i32.const 1
    i32.ge_s
    if
        call $gc_collect_old
    end
)

(func $gc_collect (param $kind i32)
    ;;@ALLOC_LOCK_ACQUIRE@
    local.get $kind
    call $__gc_collect_locked
    ;;@ALLOC_LOCK_RELEASE@
)

(func $gc_collect_ephemeral
    i32.const 0
    call $gc_collect
)

(func $gc_collect_full
    i32.const 2
    call $gc_collect
)

;; Old-space / LOH allocate: prefer a freelist block of the right size class, else bump.
(func $__gc_alloc_old (param $payload i32) (param $tag i32) (param $meta i32) (result i32)
    (local $size i32)
    (local $block i32)
    (local $new_heap i32)
    (local $idx i32)
    (local $head_addr i32)
    (local $next i32)
    local.get $payload
    i32.const 3
    i32.add
    i32.const -4
    i32.and
    i32.const 12
    i32.add
    local.set $size
    local.get $size
    call $size_class
    local.set $idx
    local.get $idx
    call $freelist_head_addr
    local.set $head_addr
    local.get $head_addr
    i32.load
    local.set $block
    (block $from_free
        local.get $block
        i32.eqz
        br_if $from_free
        ;; Exact size-class match: take head. Huge list may need first-fit; for now only
        ;; reuse when the stored block size equals the request (power-of-two classes match).
        local.get $block
        i32.load
        local.get $size
        i32.ne
        br_if $from_free
        local.get $block
        i32.const 4
        i32.add
        i32.load
        local.set $next
        local.get $head_addr
        local.get $next
        i32.store
        local.get $block
        local.get $size
        i32.store
        local.get $block
        i32.const 4
        i32.add
        local.get $tag
        i32.store
        local.get $block
        i32.const 8
        i32.add
        local.get $meta
        i32.const {GC_META_FREE}
        i32.const -1
        i32.xor
        i32.and
        i32.store
        ;;@DEBUG_ALLOC_COUNT@
        local.get $block
        i32.const 12
        i32.add
        return
    )
    i32.const {HEAP_PTR_ADDR}
    i32.load
    local.set $block
    local.get $block
    local.get $size
    i32.add
    local.set $new_heap
    local.get $new_heap
    memory.size
    i32.const 16
    i32.shl
    i32.gt_u
    if
        local.get $new_heap
        i32.const 65535
        i32.add
        i32.const 16
        i32.shr_u
        memory.size
        i32.sub
        memory.grow
        i32.const -1
        i32.eq
        (if (then unreachable))
    end
    i32.const {HEAP_PTR_ADDR}
    local.get $new_heap
    i32.store
    local.get $block
    local.get $size
    i32.store
    local.get $block
    i32.const 4
    i32.add
    local.get $tag
    i32.store
    local.get $block
    i32.const 8
    i32.add
    local.get $meta
    i32.store
    i32.const {GC_OLD_BYTES_ADDR}
    i32.const {GC_OLD_BYTES_ADDR}
    i32.load
    local.get $size
    i32.add
    i32.store
    ;;@DEBUG_ALLOC_COUNT@
    local.get $block
    i32.const 12
    i32.add
)

;; Managed allocator (exported as `$malloc`): fast Gen0 bump into the nursery; large payloads go
;; straight to LOH. Nursery full → ephemeral collect then retry, else Gen1. The lock is held for
;; the whole path so `$__gc_collect_locked` must not re-acquire it.
(func $malloc (param $payload i32) (param $tag i32) (result i32)
    (local $size i32)
    (local $block i32)
    (local $bump i32)
    (local $end i32)
    ;;@ALLOC_LOCK_ACQUIRE@
    ;; Large object: skip the nursery entirely.
    local.get $payload
    i32.const {LOH_THRESHOLD}
    i32.ge_u
    if
        local.get $payload
        local.get $tag
        i32.const {GC_GEN_LOH}
        call $__gc_alloc_old
        local.set $block
        ;;@ALLOC_LOCK_RELEASE@
        local.get $block
        return
    end
    ;; Aligned block size (matches `$__gc_alloc_old`: payload align 4 + 12 header).
    local.get $payload
    i32.const 3
    i32.add
    i32.const -4
    i32.and
    i32.const 12
    i32.add
    local.set $size
    ;; Try nursery bump. Remset overflow sets `GC_REQUEST_ADDR` but is still correct if we
    ;; wait until the nursery fills: `$gc_scan_old_for_young` covers unrecorded old→young edges.
    i32.const {NURSERY_BUMP_ADDR}
    i32.load
    local.set $bump
    global.get $__gc_nursery_end
    local.set $end
    local.get $bump
    local.get $size
    i32.add
    local.get $end
    i32.gt_u
    if
        ;; Nursery full: ephemeral collect (Gen0), then retry.
        i32.const 0
        call $__gc_collect_locked
        i32.const {NURSERY_BUMP_ADDR}
        i32.load
        local.set $bump
        local.get $bump
        local.get $size
        i32.add
        global.get $__gc_nursery_end
        i32.gt_u
        if
            local.get $payload
            local.get $tag
            i32.const {GC_GEN1}
            call $__gc_alloc_old
            local.set $block
            ;;@ALLOC_LOCK_RELEASE@
            local.get $block
            return
        end
    end
    local.get $bump
    local.set $block
    i32.const {NURSERY_BUMP_ADDR}
    local.get $block
    local.get $size
    i32.add
    i32.store
    ;; Header: [size][tag][gc_meta] with Gen0 gen bits.
    local.get $block
    local.get $size
    i32.store
    local.get $block
    i32.const 4
    i32.add
    local.get $tag
    i32.store
    local.get $block
    i32.const 8
    i32.add
    i32.const {GC_GEN0}
    i32.store
    ;;@DEBUG_ALLOC_COUNT@
    local.get $block
    i32.const 12
    i32.add
    local.set $block
    ;;@ALLOC_LOCK_RELEASE@
    local.get $block
)

;; Initialize nursery + GC tables. Called once when HEAP_PTR CAS wins (from `$__runtime_init` body).
(func $__gc_init (param $heap_base i32)
    (local $old i32)
    (local $tables i32)
    (local $bytes i32)
    i32.const {NURSERY_START_ADDR}
    local.get $heap_base
    i32.store
    i32.const {NURSERY_END_ADDR}
    local.get $heap_base
    i32.const {NURSERY_SIZE}
    i32.add
    i32.store
    i32.const {NURSERY_BUMP_ADDR}
    local.get $heap_base
    i32.store
    local.get $heap_base
    i32.const {NURSERY_SIZE}
    i32.add
    local.set $old
    ;; root table + remset + mark stack sit between nursery and old-space objects so
    ;; `$gc_sweep_old` never interprets bookkeeping words as heap block headers.
    i32.const {GC_ROOT_TABLE_PTR_ADDR}
    local.get $old
    i32.store
    local.get $old
    i32.const {GC_ROOT_TABLE_CAP}
    i32.const 2
    i32.shl
    i32.add
    local.set $tables
    i32.const {GC_REMSET_TABLE_PTR_ADDR}
    local.get $tables
    i32.store
    local.get $tables
    i32.const {GC_REMEMBERED_CAP}
    i32.const 2
    i32.shl
    i32.add
    local.set $tables
    i32.const {GC_MARK_STACK_BASE_ADDR}
    local.get $tables
    i32.store
    i32.const {GC_MARK_STACK_PTR_ADDR}
    local.get $tables
    i32.store
    local.get $tables
    i32.const {GC_MARK_STACK_CAP}
    i32.const 2
    i32.shl
    i32.add
    local.set $tables
    ;; ensure pages
    local.get $tables
    memory.size
    i32.const 16
    i32.shl
    i32.gt_u
    if
        local.get $tables
        i32.const 65535
        i32.add
        i32.const 16
        i32.shr_u
        memory.size
        i32.sub
        memory.grow
        i32.const -1
        i32.eq
        (if (then unreachable))
    end
    i32.const {OLD_START_ADDR}
    local.get $tables
    i32.store
    i32.const {HEAP_PTR_ADDR}
    local.get $tables
    i32.store
    i32.const {GC_ROOT_COUNT_ADDR}
    i32.const 0
    i32.store
    i32.const {GC_REMSET_COUNT_ADDR}
    i32.const 0
    i32.store
    i32.const {GC_REMSET_OVERFLOW_ADDR}
    i32.const 0
    i32.store
    i32.const {GC_FINALIZER_HEAD_ADDR}
    i32.const 0
    i32.store
    i32.const {GC_OLD_BYTES_ADDR}
    i32.const 0
    i32.store
    i32.const {GC_REQUEST_ADDR}
    i32.const 0
    i32.store
    i32.const {GC_EPOCH_ADDR}
    i32.const 0
    i32.store
    call $__gc_cache_bounds
)

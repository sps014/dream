;; --- Segregated free-list (slab) allocator + GC entry points ------------------------------------
;;
;; Managed allocations go through `$gc_alloc` (Gen0 nursery / LOH). `$malloc` is the public entry
;; that acquires the alloc lock and calls `$gc_alloc` with Gen0 metadata.
;; `$free` / freelists reclaim blocks after GC sweep or for `@unsafe` Buffer/Pointer paths.
;;
;; Free-list heads live in a fixed, zero-initialized low-memory table (so every list starts empty).
;; Layout is constrained by `abi.rs`: `ALLOC_LOCK_ADDR=44`, `HEAP_PTR_ADDR=48`, and
;; `THREAD_ID_COUNTER_ADDR=52` occupy those words — freelist heads must NOT land on 44/48/52.
;;
;;   idx 0..8   @  4 + idx*4              -> O(1) classes 16,32,...,4096
;;   idx 9..12  @  56 + (idx-9)*4         -> O(1) large classes 8192,16384,32768,65536
;;   idx > 12   @  72                     -> first-fit huge list (blocks larger than 65536)
;;
;; Heap block layout: [size:i32][tag:i32][gc_meta:i32], data at block+12. While a block sits on a
;; free list, block+4 (the tag word) holds the next-free pointer; block+0 keeps the block size.

(func $size_class (param $size i32) (result i32)
    (local $s i32)
    local.get $size
    local.set $s
    local.get $s
    i32.const 16
    i32.lt_s
    (if (then i32.const 16 local.set $s))
    i32.const 28
    local.get $s
    i32.const 1
    i32.sub
    i32.clz
    i32.sub
)

(func $freelist_head_addr (param $idx i32) (result i32)
    local.get $idx
    i32.const 8
    i32.le_s
    if
        local.get $idx
        i32.const 2
        i32.shl
        i32.const 4
        i32.add
        return
    end
    local.get $idx
    i32.const 12
    i32.le_s
    if
        local.get $idx
        i32.const 9
        i32.sub
        i32.const 2
        i32.shl
        i32.const 56
        i32.add
        return
    end
    i32.const 72
)

(func $__alloc_lock_acquire
    (loop $acquire
        i32.const {ALLOC_LOCK_ADDR}
        i32.const 0
        i32.const 1
        i32.atomic.rmw.cmpxchg
        i32.const 0
        i32.ne
        br_if $acquire
    )
)

(func $__alloc_lock_release
    i32.const {ALLOC_LOCK_ADDR}
    i32.const 0
    i32.atomic.store
)

(func $malloc (param $size i32) (param $tag i32) (result i32)
    (local $result i32)
    ;;@ALLOC_LOCK_ACQUIRE@
    local.get $size
    local.get $tag
    i32.const 0
    call $gc_alloc
    local.set $result
    ;;@ALLOC_LOCK_RELEASE@
    local.get $result
)

;; Unmanaged / post-sweep free. Does not run finalizers (collector already did).
(func $free (param $ptr i32)
    local.get $ptr
    i32.eqz
    br_if 0
    ;;@ALLOC_LOCK_ACQUIRE@
    local.get $ptr
    call $__free_locked
    ;;@ALLOC_LOCK_RELEASE@
)

(func $__free_locked (param $ptr i32)
    (local $block_start i32)
    (local $idx i32)
    (local $head_addr i32)
    (local $size i32)
    (local $meta i32)
    local.get $ptr
    i32.eqz
    br_if 0
    local.get $ptr
    i32.const 12
    i32.sub
    local.set $block_start
    local.get $block_start
    i32.load
    local.set $size
    local.get $size
    i32.eqz
    br_if 0
    ;;@DEBUG_FREE_COUNT@
    local.get $block_start
    i32.const 8
    i32.add
    i32.load
    local.set $meta
    local.get $block_start
    i32.const 8
    i32.add
    local.get $meta
    i32.const {GC_META_FREE}
    i32.or
    i32.store
    local.get $size
    call $size_class
    local.set $idx
    local.get $idx
    call $freelist_head_addr
    local.set $head_addr
    local.get $block_start
    i32.const 4
    i32.add
    local.get $head_addr
    i32.load
    i32.store
    local.get $head_addr
    local.get $block_start
    i32.store
    local.get $block_start
    global.set $free_list_head
)

(func $realloc (param $ptr i32) (param $new_size i32) (param $tag i32) (result i32)
    (local $block_start i32)
    (local $old_total i32)
    (local $new_total i32)
    (local $new_ptr i32)
    (local $old_payload i32)
    (local $copy_size i32)
    local.get $ptr
    i32.eqz
    (if (result i32)
        (then
            local.get $new_size
            local.get $tag
            call $malloc
        )
        (else
            local.get $ptr
            i32.const 12
            i32.sub
            local.set $block_start
            local.get $block_start
            i32.load
            local.set $old_total
            local.get $new_size
            i32.const 3
            i32.add
            i32.const -4
            i32.and
            i32.const 12
            i32.add
            local.set $new_total
            local.get $new_total
            local.get $old_total
            i32.le_u
            (if (result i32)
                (then local.get $ptr)
                (else
                    local.get $new_size
                    local.get $tag
                    call $malloc
                    local.set $new_ptr
                    local.get $old_total
                    i32.const 12
                    i32.sub
                    local.set $old_payload
                    local.get $old_payload
                    local.get $new_size
                    i32.lt_u
                    (if (result i32)
                        (then local.get $old_payload)
                        (else local.get $new_size)
                    )
                    local.set $copy_size
                    local.get $new_ptr
                    local.get $ptr
                    local.get $copy_size
                    memory.copy
                    local.get $ptr
                    call $free
                    local.get $new_ptr
                )
            )
        )
    )
)

(func $object_tag (param $ptr i32) (result i32)
    local.get $ptr
    i32.eqz
    (if (result i32)
        (then i32.const 0)
        (else
            local.get $ptr
            i32.const 8
            i32.sub
            i32.load
        )
    )
)

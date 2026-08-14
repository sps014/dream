;; --- Segregated free-list allocator (no GC) ----------------------------------------------------
;;
;; `$malloc` / `$free` / `$realloc` manage `[size:i32][tag:i32][reserved:i32]` blocks; the data
;; pointer is block+12. Immortal interned strings use size 0 and `$free` ignores them.
;;
;; Free-list heads live in a fixed, zero-initialized low-memory table.
;; Layout is constrained by `abi.rs`: `ALLOC_LOCK_ADDR=44`, `HEAP_PTR_ADDR=48`, and
;; `THREAD_ID_COUNTER_ADDR=52` occupy those words — freelist heads must NOT land on 44/48/52.
;;
;;   idx 0..8   @  4 + idx*4              -> O(1) classes 16,32,...,4096
;;   idx 9..12  @  56 + (idx-9)*4         -> O(1) large classes 8192,16384,32768,65536
;;   idx > 12   @  72                     -> first-fit huge list (blocks larger than 65536)

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

(func $class_bytes (param $idx i32) (result i32)
    local.get $idx
    i32.const 8
    i32.le_s
    if (result i32)
        i32.const 16
        local.get $idx
        i32.shl
    else
        local.get $idx
        i32.const 12
        i32.le_s
        if (result i32)
            i32.const 8192
            local.get $idx
            i32.const 9
            i32.sub
            i32.shl
        else
            i32.const 0
        end
    end
)

(func $__heap_ptr_get (result i32)
    ;;@HEAP_PTR_GET_BODY@
)

(func $__heap_ptr_set (param $p i32)
    ;;@HEAP_PTR_SET_BODY@
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
    local.get $ptr
    i32.eqz
    br_if 0
    ;; Arena bump allocations are not individually freed.
    global.get $__arena_base
    i32.eqz
    i32.eqz
    if
        local.get $ptr
        global.get $__arena_base
        i32.ge_u
        local.get $ptr
        global.get $__arena_end
        i32.lt_u
        i32.and
        if
            return
        end
    end
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
    ;; Idempotent free: reserved==1 is already on a freelist. reserved==2 is `$dream_drop` in
    ;; progress and must still be inserted (then marked 1).
    local.get $block_start
    i32.const 8
    i32.add
    i32.load
    i32.const 1
    i32.eq
    br_if 0
    local.get $block_start
    i32.const 8
    i32.add
    i32.const 1
    i32.store
    ;;@DEBUG_FREE_COUNT@
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

(func $malloc (param $payload i32) (param $tag i32) (result i32)
    (local $size i32)
    (local $block i32)
    (local $new_heap i32)
    (local $idx i32)
    (local $head_addr i32)
    (local $next i32)
    global.get $__arena_base
    i32.eqz
    i32.eqz
    if
        local.get $payload
        local.get $tag
        call $__arena_malloc
        return
    end
    ;;@ALLOC_LOCK_ACQUIRE@
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
    i32.const 12
    i32.le_s
    if
        local.get $idx
        call $class_bytes
        local.set $size
    end
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
        i32.const 0
        i32.store
        ;;@DEBUG_ALLOC_COUNT@
        local.get $block
        i32.const 12
        i32.add
        ;;@ALLOC_LOCK_RELEASE@
        return
    )
    ;;@HEAP_PTR_GET@
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
    ;;@HEAP_PTR_SET@
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
    i32.const 0
    i32.store
    ;;@DEBUG_ALLOC_COUNT@
    local.get $block
    i32.const 12
    i32.add
    ;;@ALLOC_LOCK_RELEASE@
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

;; Kept so `Debug.gc_collect()` still links; no collector remains.
(func $gc_collect_full)

(global $__arena_base (mut i32) (i32.const 0))
(global $__arena_bump (mut i32) (i32.const 0))
(global $__arena_end (mut i32) (i32.const 0))
(global $heap_ptr (mut i32) (i32.const 0))

(func $__arena_malloc (param $payload i32) (param $tag i32) (result i32)
    (local $size i32)
    (local $block i32)
    (local $new_bump i32)
    local.get $payload
    i32.const 3
    i32.add
    i32.const -4
    i32.and
    i32.const 12
    i32.add
    local.set $size
    global.get $__arena_bump
    local.set $block
    local.get $block
    local.get $size
    i32.add
    local.set $new_bump
    local.get $new_bump
    global.get $__arena_end
    i32.gt_u
    if
        unreachable
    end
    local.get $new_bump
    global.set $__arena_bump
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
    i32.const 0
    i32.store
    local.get $block
    i32.const 12
    i32.add
)

;; `size` is the usable bump region in bytes (at least 4096). The GPA slab is `size+12` so the
;; first 12 payload bytes can hold the previous (base, bump, end) for nested `with`.
(func $arena_enter (param $size i32)
    (local $prev_base i32)
    (local $prev_bump i32)
    (local $prev_end i32)
    (local $slab i32)
    local.get $size
    i32.const 4096
    i32.lt_s
    if
        i32.const 4096
        local.set $size
    end
    global.get $__arena_base
    local.set $prev_base
    global.get $__arena_bump
    local.set $prev_bump
    global.get $__arena_end
    local.set $prev_end
    i32.const 0
    global.set $__arena_base
    local.get $size
    i32.const 12
    i32.add
    i32.const 0
    call $malloc
    local.set $slab
    local.get $slab
    local.get $prev_base
    i32.store
    local.get $slab
    i32.const 4
    i32.add
    local.get $prev_bump
    i32.store
    local.get $slab
    i32.const 8
    i32.add
    local.get $prev_end
    i32.store
    local.get $slab
    global.set $__arena_base
    local.get $slab
    i32.const 12
    i32.add
    global.set $__arena_bump
    local.get $slab
    local.get $size
    i32.const 12
    i32.add
    i32.add
    global.set $__arena_end
)

(func $arena_exit
    (local $slab i32)
    (local $prev_base i32)
    (local $prev_bump i32)
    (local $prev_end i32)
    global.get $__arena_base
    local.set $slab
    local.get $slab
    i32.eqz
    br_if 0
    local.get $slab
    i32.load
    local.set $prev_base
    local.get $slab
    i32.const 4
    i32.add
    i32.load
    local.set $prev_bump
    local.get $slab
    i32.const 8
    i32.add
    i32.load
    local.set $prev_end
    i32.const 0
    global.set $__arena_base
    local.get $slab
    call $free
    local.get $prev_base
    global.set $__arena_base
    local.get $prev_bump
    global.set $__arena_bump
    local.get $prev_end
    global.set $__arena_end
)

(func $__drop_array (param $ptr i32)
    (local $n i32)
    (local $i i32)
    (local $slot i32)
    local.get $ptr
    i32.eqz
    br_if 0
    local.get $ptr
    i32.load
    local.set $n
    i32.const 0
    local.set $i
    (block $done
        (loop $elems
            local.get $i
            local.get $n
            i32.ge_s
            br_if $done
            local.get $ptr
            i32.const 4
            i32.add
            local.get $i
            i32.const 2
            i32.shl
            i32.add
            i32.load
            call $dream_drop
            local.get $i
            i32.const 1
            i32.add
            local.set $i
            br $elems
        )
    )
    local.get $ptr
    call $free
)

(func $__gpa_check_leaks
    global.get $live_objects
    i32.eqz
    br_if 0
    unreachable
)

;; --- `weak` side table (GC) ---------------------------------------------------------------------
;;
;; A `weak` field must not keep its referent alive. The collector finds unreachable objects via
;; tracing; before finalizers run / the block is reclaimed, `$weak_clear_all` poisons every live
;; `weak` slot that watched that referent so a later read observes `Option.None` instead of a
;; dangling pointer. See `docs/language/memory.md` and `docs/compiler/12-tiered-gc.md`.
;;
;; The table is one unbucketed singly linked list of small heap nodes (private allocations, tag 0,
;; never traced as program objects — managed exclusively by the three functions below via
;; `$malloc` / `$__free_locked`). O(n) per operation is fine: `weak` fields are rare.
;;
;; `$weak_clear_all` is called from GC finalizer / sweep paths while the allocator lock is already
;; held, so it must free watch nodes with `$__free_locked` (not `$free`, which would re-acquire the
;; lock and deadlock under threads).
;;
;; Node layout (20 bytes, `$malloc(20, 0)`):
;;   +0  target : i32   -- the referent this registration watches
;;   +4  slot   : i32   -- address of the private weak-box's discriminant word
;;   +8  kind   : i32   -- 0 = weak→None; 1 = legacy unowned (unused; `unowned` is deleted)
;;   +12 extra  : i32   -- `Option.None` discriminant written to `slot` on poison
;;   +16 next   : i32   -- next node, or 0

(global $weak_list_head (mut i32) (i32.const 0))

;; Registers `slot` as watching `target`. A no-op when `target` is null. Called once per store into
;; a live `weak` field (kind 0).
(func $weak_register (param $target i32) (param $slot i32) (param $kind i32) (param $extra i32)
    (local $node i32)
    local.get $target
    i32.eqz
    br_if 0
    i32.const 20
    i32.const 0
    call $malloc
    local.set $node
    local.get $node
    local.get $target
    i32.store
    local.get $node
    i32.const 4
    i32.add
    local.get $slot
    i32.store
    local.get $node
    i32.const 8
    i32.add
    local.get $kind
    i32.store
    local.get $node
    i32.const 12
    i32.add
    local.get $extra
    i32.store
    local.get $node
    i32.const 16
    i32.add
    global.get $weak_list_head
    i32.store
    local.get $node
    global.set $weak_list_head
)

;; Removes the (unique) registration for `(target, slot)`, if any, and frees its node. Called before
;; a `weak` slot is overwritten or torn down.
(func $weak_unregister (param $target i32) (param $slot i32)
    (local $prev i32)
    (local $curr i32)
    (local $next i32)
    local.get $target
    i32.eqz
    br_if 0
    i32.const 0
    local.set $prev
    global.get $weak_list_head
    local.set $curr
    (block $done
        (loop $scan
            local.get $curr
            i32.eqz
            br_if $done
            local.get $curr
            i32.const 16
            i32.add
            i32.load
            local.set $next
            local.get $curr
            i32.load
            local.get $target
            i32.ne
            (if
                (then
                    local.get $curr
                    local.set $prev
                    local.get $next
                    local.set $curr
                    br $scan
                )
            )
            local.get $curr
            i32.const 4
            i32.add
            i32.load
            local.get $slot
            i32.ne
            (if
                (then
                    local.get $curr
                    local.set $prev
                    local.get $next
                    local.set $curr
                    br $scan
                )
            )
            local.get $prev
            i32.eqz
            (if
                (then
                    local.get $next
                    global.set $weak_list_head
                )
                (else
                    local.get $prev
                    i32.const 16
                    i32.add
                    local.get $next
                    i32.store
                )
            )
            local.get $curr
            call $free
            br $done
        )
    )
)

;; Called from GC when `target` is unreachable: poisons every live `weak` slot watching it
;; (kind 0 → `Option.None`), unregistering and freeing each watch node. Kind 1 is legacy dead code.
;; Safe under the allocator lock (uses `$__free_locked`).
(func $weak_clear_all (param $target i32)
    (local $prev i32)
    (local $curr i32)
    (local $next i32)
    (local $slot i32)
    (local $kind i32)
    local.get $target
    i32.eqz
    br_if 0
    i32.const 0
    local.set $prev
    global.get $weak_list_head
    local.set $curr
    (block $done
        (loop $scan
            local.get $curr
            i32.eqz
            br_if $done
            local.get $curr
            i32.const 16
            i32.add
            i32.load
            local.set $next
            local.get $curr
            i32.load
            local.get $target
            i32.eq
            (if
                (then
                    local.get $curr
                    i32.const 4
                    i32.add
                    i32.load
                    local.set $slot
                    local.get $curr
                    i32.const 8
                    i32.add
                    i32.load
                    local.set $kind
                    local.get $kind
                    i32.eqz
                    (if
                        (then
                            ;; weak: reset private weak-box discriminant to None; zero payload.
                            local.get $slot
                            local.get $curr
                            i32.const 12
                            i32.add
                            i32.load
                            i32.store
                            local.get $slot
                            i32.const 4
                            i32.add
                            i32.const 0
                            i32.store
                        )
                        (else
                            ;; legacy unowned: poison the field word (unused under GC).
                            local.get $slot
                            i32.const 0
                            i32.store
                        )
                    )
                    local.get $prev
                    i32.eqz
                    (if
                        (then
                            local.get $next
                            global.set $weak_list_head
                        )
                        (else
                            local.get $prev
                            i32.const 16
                            i32.add
                            local.get $next
                            i32.store
                        )
                    )
                    local.get $curr
                    call $__free_locked
                )
                (else
                    local.get $curr
                    local.set $prev
                )
            )
            local.get $next
            local.set $curr
            br $scan
        )
    )
)

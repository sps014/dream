(type $dream_poll_t (func (param i32) (result i32)))
(global $rq_head (mut i32) (i32.const 0))
(global $rq_tail (mut i32) (i32.const 0))
(global $timer_head (mut i32) (i32.const 0))
(global $vclock (mut i32) (i32.const 0))
;; Owner instance only: write-through so a worker Gen0 can trace the owner's queues
;; (WASM globals are per-instance; linear memory is shared).
(global $__sched_publish (mut i32) (i32.const 0))
(func $rq_head_get (result i32)
    global.get $rq_head)
(func $rq_head_set (param $v i32)
    local.get $v
    global.set $rq_head
    global.get $__sched_publish
    if
        i32.const {RQ_HEAD_ADDR}
        local.get $v
        i32.store
    end)
(func $rq_tail_get (result i32)
    global.get $rq_tail)
(func $rq_tail_set (param $v i32)
    local.get $v
    global.set $rq_tail
    global.get $__sched_publish
    if
        i32.const {RQ_TAIL_ADDR}
        local.get $v
        i32.store
    end)
(func $timer_head_get (result i32)
    global.get $timer_head)
(func $timer_head_set (param $v i32)
    local.get $v
    global.set $timer_head
    global.get $__sched_publish
    if
        i32.const {TIMER_HEAD_ADDR}
        local.get $v
        i32.store
    end)
(func $vclock_get (result i32)
    global.get $vclock)
(func $vclock_set (param $v i32)
    local.get $v
    global.set $vclock
    global.get $__sched_publish
    if
        i32.const {VCLOCK_ADDR}
        local.get $v
        i32.store
    end)
(func $wb_off (param $obj i32) (param $off i32) (param $val i32)
    local.get $obj
    local.get $off
    i32.add
    local.get $val
    call $write_barrier)
(func $gc_update_or_mark (param $slot i32)
    global.get $gc_trace_mode
    i32.const 1
    i32.eq
    if
        local.get $slot
        i32.load
        call $gc_mark_object
    else
        local.get $slot
        call $gc_update_slot
    end)

;; Old Futures are not evacuated this Gen0, so `$gc_update_slot` on a slot holding one
;; does not walk `$F_RESULT`. Young results (worker-reply strings) would otherwise die.
(func $gc_touch_old_future (param $ptr i32)
    local.get $ptr
    i32.eqz
    br_if 0
    local.get $ptr
    call $__gc_in_nursery
    br_if 0
    local.get $ptr
    i32.const 12
    i32.lt_u
    br_if 0
    local.get $ptr
    call $object_tag
    br_if 0
    local.get $ptr
    i32.const 12
    i32.sub
    i32.load
    i32.const 32
    i32.le_u
    br_if 0
    local.get $ptr
    i32.const {F_RESULT}
    i32.add
    call $gc_update_or_mark
    local.get $ptr
    i32.const {F_AWAITING}
    i32.add
    call $gc_update_or_mark
    local.get $ptr
    i32.const {F_CHILDREN}
    i32.add
    call $gc_update_or_mark
    local.get $ptr
    i32.const {F_RESULTS}
    i32.add
    call $gc_update_or_mark
)

;; Conservatively update every i32 word in a Future frame (header pointer fields + user slots).
;; Tag 0 is shared with 8-byte funcboxes; only oversized tag-0 blocks reach here.
(func $gc_trace_future_frame (param $ptr i32)
    (local $slot i32)
    (local $end i32)
    local.get $ptr
    i32.eqz
    br_if 0
    local.get $ptr
    i32.const 12
    i32.sub
    i32.load
    i32.const 12
    i32.sub
    local.get $ptr
    i32.add
    local.set $end
    local.get $ptr
    i32.const 8
    i32.add
    local.set $slot
    ;; Pointer fields in the Future header; skip scalars (state/status/poll/kind/counts/due/wide).
    local.get $ptr
    i32.const {F_RESULT}
    i32.add
    call $gc_update_or_mark
    local.get $ptr
    i32.const {F_WAKER}
    i32.add
    call $gc_update_or_mark
    local.get $ptr
    i32.const {F_AWAITING}
    i32.add
    call $gc_update_or_mark
    local.get $ptr
    i32.const {F_CHILDREN}
    i32.add
    call $gc_update_or_mark
    local.get $ptr
    i32.const {F_RESULTS}
    i32.add
    call $gc_update_or_mark
    local.get $ptr
    i32.const {F_NEXT}
    i32.add
    call $gc_update_or_mark
    local.get $ptr
    i32.const {F_SLOTS}
    i32.add
    local.set $slot
    (loop $words
        local.get $slot
        local.get $end
        i32.ge_u
        br_if 1
        local.get $slot
        call $gc_update_or_mark
        local.get $slot
        i32.load
        call $gc_touch_old_future
        local.get $slot
        i32.const 4
        i32.add
        local.set $slot
        br $words
    )
)

(func $gc_scan_future_chain (param $f i32)
    (local $w i32)
    (loop $q
        local.get $f
        i32.eqz
        br_if 1
        local.get $f
        call $gc_trace_future_frame
        local.get $f
        i32.load offset={F_WAKER}
        local.set $w
        local.get $w
        if
            local.get $w
            call $gc_trace_future_frame
        end
        local.get $f
        i32.load offset={F_AWAITING}
        local.set $w
        local.get $w
        if
            local.get $w
            call $gc_trace_future_frame
        end
        local.get $f
        i32.const {F_NEXT}
        i32.add
        call $gc_update_slot
        local.get $f
        i32.load offset={F_NEXT}
        local.set $f
        br $q
    )
)

(func $gc_scan_scheduler_roots
    (local $f i32)
    call $rq_head_get
    call $gc_forward
    local.set $f
    local.get $f
    call $rq_head_set
    call $rq_tail_get
    call $gc_forward
    call $rq_tail_set
    local.get $f
    call $gc_scan_future_chain
    call $timer_head_get
    call $gc_forward
    local.set $f
    local.get $f
    call $timer_head_set
    local.get $f
    call $gc_scan_future_chain
)

(func $dream_new_future (param $size i32) (param $poll i32) (param $kind i32) (result i32)
    (local $p i32)
    local.get $size
    i32.const 0
    call $malloc
    local.set $p
    local.get $p
    i32.const 0
    local.get $size
    memory.fill
    local.get $p
    local.get $poll
    i32.store offset={F_POLL}
    local.get $p
    local.get $kind
    i32.store offset={F_KIND}
    local.get $p
)
(func $dream_enqueue (param $f i32)
    local.get $f
    i32.eqz
    br_if 0
    local.get $f
    i32.load offset={F_QUEUED}
    br_if 0
    local.get $f
    i32.const 1
    i32.store offset={F_QUEUED}
    local.get $f
    i32.const 0
    i32.store offset={F_NEXT}
    call $rq_tail_get
    i32.eqz
    (if
        (then
            local.get $f
            call $rq_head_set
            local.get $f
            call $rq_tail_set
        )
        (else
            call $rq_tail_get
            local.get $f
            i32.store offset={F_NEXT}
            call $rq_tail_get
            i32.const {F_NEXT}
            local.get $f
            call $wb_off
            local.get $f
            call $rq_tail_set
        )
    )
)
(func $dream_complete (param $f i32) (param $res i32)
    (local $w i32)
    local.get $f
    i32.load offset={F_STATUS}
    br_if 0
    local.get $f
    local.get $res
    i32.store offset={F_RESULT}
    local.get $f
    i32.const {F_RESULT}
    local.get $res
    call $wb_off
    local.get $f
    i32.const 1
    i32.store offset={F_STATUS}
    local.get $f
    i32.load offset={F_WAKER}
    local.set $w
    local.get $w
    i32.eqz
    br_if 0
    local.get $f
    i32.const 0
    i32.store offset={F_WAKER}
    local.get $w
    local.get $f
    call $dream_wake
)
(func $dream_wake (param $w i32) (param $child i32)
    local.get $w
    i32.load offset={F_KIND}
    i32.eqz
    (if
        (then
            local.get $w
            call $dream_enqueue
        )
        (else
            local.get $w
            local.get $child
            call $dream_combinator_progress
        )
    )
)
(func $dream_await (param $parent i32) (param $child i32)
    local.get $child
    local.get $parent
    i32.store offset={F_WAKER}
    local.get $child
    i32.const {F_WAKER}
    local.get $parent
    call $wb_off
    local.get $child
    i32.load offset={F_STATUS}
    (if
        (then
            local.get $parent
            call $dream_enqueue
        )
    )
)
(func $dream_resolve (param $f i32) (param $res i32)
    local.get $f
    local.get $res
    call $dream_complete
)
(func $dream_cancel (param $c i32)
    (local $cur i32)
    (local $nxt i32)
    local.get $c
    i32.eqz
    br_if 0
    local.get $c
    i32.load offset={F_STATUS}
    br_if 0
    local.get $c
    i32.const {STATUS_CANCELLED}
    i32.store offset={F_STATUS}
    local.get $c
    i32.const 0
    i32.store offset={F_WAKER}
    ;; unlink $c from the timer list so a pending timer never fires it
    call $timer_head_get
    local.get $c
    i32.eq
    (if
        (then
            local.get $c
            i32.load offset={F_NEXT}
            call $timer_head_set
        )
        (else
            call $timer_head_get
            local.set $cur
            (block $unlinked
                (loop $scan
                    local.get $cur
                    i32.eqz
                    br_if $unlinked
                    local.get $cur
                    i32.load offset={F_NEXT}
                    local.set $nxt
                    local.get $nxt
                    local.get $c
                    i32.eq
                    (if
                        (then
                            local.get $cur
                            local.get $c
                            i32.load offset={F_NEXT}
                            i32.store offset={F_NEXT}
                            br $unlinked
                        )
                    )
                    local.get $nxt
                    local.set $cur
                    br $scan
                )
            )
        )
    )
)
(func $dream_set_timer (param $f i32) (param $delay i32)
    (local $due i32)
    (local $cur i32)
    (local $nxt i32)
    call $vclock_get
    local.get $delay
    i32.add
    local.set $due
    local.get $f
    local.get $due
    i32.store offset={F_DUE}
    call $timer_head_get
    i32.eqz
    (if
        (then
            local.get $f
            i32.const 0
            i32.store offset={F_NEXT}
            local.get $f
            call $timer_head_set
            return
        )
    )
    call $timer_head_get
    i32.load offset={F_DUE}
    local.get $due
    i32.gt_s
    (if
        (then
            local.get $f
            call $timer_head_get
            i32.store offset={F_NEXT}
            local.get $f
            call $timer_head_set
            return
        )
    )
    call $timer_head_get
    local.set $cur
    (block $done
        (loop $scan
            local.get $cur
            i32.load offset={F_NEXT}
            local.set $nxt
            local.get $nxt
            i32.eqz
            br_if $done
            local.get $nxt
            i32.load offset={F_DUE}
            local.get $due
            i32.gt_s
            br_if $done
            local.get $nxt
            local.set $cur
            br $scan
        )
    )
    local.get $f
    local.get $cur
    i32.load offset={F_NEXT}
    i32.store offset={F_NEXT}
    local.get $cur
    local.get $f
    i32.store offset={F_NEXT}
)
(func $dream_run_loop
    (local $f i32)
    (local $t i32)
    (block $alldone
        (loop $outer
            (block $drained
                (loop $drain
                    call $rq_head_get
                    local.set $f
                    local.get $f
                    i32.eqz
                    br_if $drained
                    local.get $f
                    i32.load offset={F_NEXT}
                    call $rq_head_set
                    call $rq_head_get
                    i32.eqz
                    (if
                        (then
                            i32.const 0
                            call $rq_tail_set
                        )
                    )
                    local.get $f
                    i32.const 0
                    i32.store offset={F_QUEUED}
                    local.get $f
                    i32.const 0
                    i32.store offset={F_NEXT}
                    ;; a cancelled/settled future may still sit in the ready queue; never poll it
                    local.get $f
                    i32.load offset={F_STATUS}
                    br_if $drain
                    local.get $f
                    local.get $f
                    i32.load offset={F_POLL}
                    call_indirect (type $dream_poll_t)
                    drop
                    br $drain
                )
            )
            call $timer_head_get
            i32.eqz
            br_if $alldone
            call $timer_head_get
            i32.load offset={F_DUE}
            call $vclock_set
            (block $timers_done
                (loop $tloop
                    call $timer_head_get
                    local.set $t
                    local.get $t
                    i32.eqz
                    br_if $timers_done
                    local.get $t
                    i32.load offset={F_DUE}
                    call $vclock_get
                    i32.gt_s
                    br_if $timers_done
                    local.get $t
                    i32.load offset={F_NEXT}
                    call $timer_head_set
                    local.get $t
                    i32.const 0
                    i32.store offset={F_NEXT}
                    local.get $t
                    i32.const 0
                    call $dream_complete
                    br $tloop
                )
            )
            br $outer
        )
    )
)
(func $dream_combinator_progress (param $w i32) (param $child i32)
    (local $n i32)
    (local $i i32)
    (local $arr i32)
    (local $c i32)
    local.get $w
    i32.load offset={F_KIND}
    i32.const {KIND_ALL}
    i32.eq
    (if
        (then
            local.get $w
            local.get $w
            i32.load offset={F_REMAINING}
            i32.const 1
            i32.sub
            i32.store offset={F_REMAINING}
            local.get $w
            i32.load offset={F_REMAINING}
            i32.eqz
            (if
                (then
                    local.get $w
                    i32.load offset={F_COUNT}
                    local.set $n
                    i32.const 4
                    local.get $n
                    i32.const 4
                    i32.mul
                    i32.add
                    i32.const {tag_array}
                    call $malloc
                    local.set $arr
                    local.get $arr
                    local.get $n
                    i32.store
                    i32.const 0
                    local.set $i
                    (block $fdone
                        (loop $f
                            local.get $i
                            local.get $n
                            i32.ge_s
                            br_if $fdone
                            local.get $w
                            i32.load offset={F_CHILDREN}
                            i32.const 4
                            i32.add
                            local.get $i
                            i32.const 4
                            i32.mul
                            i32.add
                            i32.load
                            local.set $c
                            local.get $arr
                            i32.const 4
                            i32.add
                            local.get $i
                            i32.const 4
                            i32.mul
                            i32.add
                            local.get $c
                            i32.load offset={F_RESULT}
                            i32.store
                            local.get $i
                            i32.const 1
                            i32.add
                            local.set $i
                            br $f
                        )
                    )
                    local.get $w
                    local.get $arr
                    i32.store offset={F_RESULTS}
                    local.get $w
                    local.get $arr
                    call $dream_complete
                )
            )
        )
        (else
            local.get $w
            i32.load offset={F_STATUS}
            i32.eqz
            (if
                (then
                    local.get $w
                    local.get $child
                    i32.load offset={F_RESULT}
                    call $dream_complete
                    ;; cancel every remaining loser, then drop the parent's strong ref on the winner
                    local.get $w
                    i32.load offset={F_COUNT}
                    local.set $n
                    local.get $w
                    i32.load offset={F_CHILDREN}
                    local.set $arr
                    i32.const 0
                    local.set $i
                    (block $cdone
                        (loop $cloop
                            local.get $i
                            local.get $n
                            i32.ge_s
                            br_if $cdone
                            local.get $arr
                            i32.const 4
                            i32.add
                            local.get $i
                            i32.const 4
                            i32.mul
                            i32.add
                            i32.load
                            local.set $c
                            local.get $c
                            local.get $child
                            i32.ne
                            (if
                                (then
                                    local.get $c
                                    call $dream_cancel
                                )
                            )
                            local.get $i
                            i32.const 1
                            i32.add
                            local.set $i
                            br $cloop
                        )
                    )
                )
            )
        )
    )
)
(func $dream_all (param $arr i32) (result i32)
    (local $w i32)
    (local $n i32)
    (local $i i32)
    (local $c i32)
    local.get $arr
    i32.load
    local.set $n
    i32.const {F_SLOTS}
    i32.const -1
    i32.const {KIND_ALL}
    call $dream_new_future
    local.set $w
    local.get $w
    local.get $arr
    i32.store offset={F_CHILDREN}
    local.get $w
    local.get $n
    i32.store offset={F_COUNT}
    local.get $w
    local.get $n
    i32.store offset={F_REMAINING}
    local.get $n
    i32.eqz
    (if
        (then
            local.get $w
            local.get $arr
            call $dream_complete
            local.get $w
            return
        )
    )
    i32.const 0
    local.set $i
    (block $done
        (loop $reg
            local.get $i
            local.get $n
            i32.ge_s
            br_if $done
            local.get $arr
            i32.const 4
            i32.add
            local.get $i
            i32.const 4
            i32.mul
            i32.add
            i32.load
            local.set $c
            local.get $c
            local.get $w
            i32.store offset={F_WAKER}
            local.get $c
            i32.load offset={F_STATUS}
            (if
                (then
                    local.get $w
                    local.get $c
                    call $dream_combinator_progress
                )
            )
            local.get $i
            i32.const 1
            i32.add
            local.set $i
            br $reg
        )
    )
    local.get $w
)
(func $dream_any (param $arr i32) (result i32)
    (local $w i32)
    (local $n i32)
    (local $i i32)
    (local $c i32)
    local.get $arr
    i32.load
    local.set $n
    i32.const {F_SLOTS}
    i32.const -1
    i32.const {KIND_ANY}
    call $dream_new_future
    local.set $w
    local.get $w
    local.get $arr
    i32.store offset={F_CHILDREN}
    local.get $w
    local.get $n
    i32.store offset={F_COUNT}
    local.get $w
    local.get $n
    i32.store offset={F_REMAINING}
    i32.const 0
    local.set $i
    (block $done
        (loop $reg
            local.get $i
            local.get $n
            i32.ge_s
            br_if $done
            local.get $arr
            i32.const 4
            i32.add
            local.get $i
            i32.const 4
            i32.mul
            i32.add
            i32.load
            local.set $c
            local.get $c
            local.get $w
            i32.store offset={F_WAKER}
            local.get $c
            i32.load offset={F_STATUS}
            (if
                (then
                    local.get $w
                    local.get $c
                    call $dream_combinator_progress
                )
            )
            local.get $i
            i32.const 1
            i32.add
            local.set $i
            br $reg
        )
    )
    local.get $w
)

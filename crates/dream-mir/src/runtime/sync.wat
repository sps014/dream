;; --- Cross-thread synchronization: thread ids, the reentrant lock-word primitive, atomic retain ---
;;
;; Backs `@shared class`'s embedded lock word, the `lock (obj) { ... }` statement, and `Lock`
;; (`src/stdlib/core/sync.dream`). Every one of these is, at the WAT level, just "acquire/release
;; the reentrant lock word at address `$addr`" — a `lock` statement's target address is `obj_ptr +
;; layout.size` (see `src/mir/abi.rs`'s `@shared class` header-extension note); `Lock` is simply an
;; `@shared class` with no fields of its own, so its lock word sits at `obj_ptr + 0`.

;; Returns a small, dense id unique to the calling thread (1, 2, 3, ...), assigned once (from the
;; shared `THREAD_ID_COUNTER_ADDR` atomic counter) and cached in the per-*instance* `$__tid` global
;; thereafter — every instance of this module (the owner + every `WebWorker`) is a distinct WASM
;; instance with its own `$__tid`, so this is naturally a one-id-per-OS-thread scheme with no host
;; import needed.
(func $__thread_id (result i32)
    (local $t i32)
    global.get $__tid
    local.set $t
    local.get $t
    i32.eqz
    (if
        (then
            i32.const {THREAD_ID_COUNTER_ADDR}
            i32.const 1
            i32.atomic.rmw.add
            i32.const 1
            i32.add
            local.set $t
            local.get $t
            global.set $__tid
        )
    )
    local.get $t
)

;; Acquires the reentrant lock word at `$addr`: `0` means free; otherwise packed as `(owner_tid <<
;; 16) | depth`. A thread not already holding the lock spins (`i32.atomic.rmw.cmpxchg`) until a
;; 0->(tid,1) exchange succeeds; the thread already holding it just bumps `depth` (only that one
;; thread can ever observe `owner == its own tid`, so the increment needs no further
;; synchronization beyond the atomic RMW itself).
(func $__lock_acquire (param $addr i32)
    (local $tid i32)
    (local $cur i32)
    call $__thread_id
    local.set $tid
    (block $done
        (loop $retry
            local.get $addr
            i32.atomic.load
            local.set $cur
            local.get $cur
            i32.eqz
            (if
                (then
                    local.get $addr
                    i32.const 0
                    local.get $tid
                    i32.const 16
                    i32.shl
                    i32.const 1
                    i32.or
                    i32.atomic.rmw.cmpxchg
                    i32.const 0
                    i32.eq
                    br_if $done
                )
                (else
                    local.get $cur
                    i32.const 16
                    i32.shr_u
                    local.get $tid
                    i32.eq
                    (if
                        (then
                            local.get $addr
                            i32.const 1
                            i32.atomic.rmw.add
                            drop
                            br $done
                        )
                    )
                )
            )
            br $retry
        )
    )
)

;; Releases one level of the reentrant lock word at `$addr`, acquired via `$__lock_acquire`: at
;; depth 1 (the outermost acquire), clears the word to `0` (free); otherwise just decrements
;; `depth`. Undefined (a caller bug, not guarded here — matches C# `Monitor`/`lock`'s own contract)
;; if called by a thread that does not currently hold the lock. Outermost release notifies one
;; waiter so `$__lock_try_acquire_for` can wake before its timeout.
(func $__lock_release (param $addr i32)
    (local $cur i32)
    local.get $addr
    i32.atomic.load
    local.set $cur
    local.get $cur
    i32.const 65535
    i32.and
    i32.const 1
    i32.eq
    (if
        (then
            local.get $addr
            i32.const 0
            i32.atomic.store
            local.get $addr
            i32.const 1
            memory.atomic.notify
            drop
        )
        (else
            local.get $addr
            i32.const 1
            i32.atomic.rmw.sub
            drop
        )
    )
)

;; One-shot acquire: returns 1 on success (including reentrant bump), 0 if another thread holds it.
(func $__lock_try_acquire (param $addr i32) (result i32)
    (local $tid i32)
    (local $cur i32)
    call $__thread_id
    local.set $tid
    local.get $addr
    i32.atomic.load
    local.set $cur
    local.get $cur
    i32.eqz
    (if (result i32)
        (then
            local.get $addr
            i32.const 0
            local.get $tid
            i32.const 16
            i32.shl
            i32.const 1
            i32.or
            i32.atomic.rmw.cmpxchg
            i32.eqz
        )
        (else
            local.get $cur
            i32.const 16
            i32.shr_u
            local.get $tid
            i32.eq
            (if (result i32)
                (then
                    local.get $addr
                    i32.const 1
                    i32.atomic.rmw.add
                    drop
                    i32.const 1
                )
                (else
                    i32.const 0
                )
            )
        )
    )
)

;; Best-effort timed acquire: retries until success or `memory.atomic.wait32` reports timeout.
;; `timeout_ms <= 0` is a single try. Remaining wait time is not shrunk across spurious wakes.
(func $__lock_try_acquire_for (param $addr i32) (param $timeout_ms i32) (result i32)
    (local $cur i32)
    (local $timeout_ns i64)
    (local $wait_res i32)
    (local $ok i32)
    local.get $timeout_ms
    i32.const 0
    i32.le_s
    (if
        (then
            local.get $addr
            call $__lock_try_acquire
            return
        )
    )
    local.get $timeout_ms
    i64.extend_i32_s
    i64.const 1000000
    i64.mul
    local.set $timeout_ns
    (block $done (result i32)
        (loop $retry
            local.get $addr
            call $__lock_try_acquire
            local.set $ok
            local.get $ok
            (if
                (then
                    local.get $ok
                    br $done
                )
            )
            local.get $addr
            i32.atomic.load
            local.set $cur
            local.get $addr
            local.get $cur
            local.get $timeout_ns
            memory.atomic.wait32
            local.set $wait_res
            local.get $wait_res
            i32.const 2
            i32.eq
            (if
                (then
                    local.get $addr
                    call $__lock_try_acquire
                    br $done
                )
            )
            br $retry
        )
        i32.const 0
    )
)

;; `Lock.acquire()`/`Lock.release()` (`src/stdlib/core/sync.dream`): `Lock` is an `@shared class`
;; with no fields of its own, so its embedded lock word sits at `this + 0` — these are thin
;; wrappers exposing `$__lock_acquire`/`$__lock_release` as callable methods (for critical sections
;; that span more than one statement/method, unlike the `lock (obj) { ... }` block sugar).
(func $shared_lock_acquire (param $lock i32)
    local.get $lock
    call $__lock_acquire
)
(func $shared_lock_release (param $lock i32)
    local.get $lock
    call $__lock_release
)
(func $shared_lock_try_acquire (param $lock i32) (result i32)
    local.get $lock
    call $__lock_try_acquire
)
(func $shared_lock_try_acquire_for (param $lock i32) (param $timeout_ms i32) (result i32)
    local.get $lock
    local.get $timeout_ms
    call $__lock_try_acquire_for
)

;; `Semaphore.acquire()`/`Semaphore.release()`: a classic counting semaphore, independent of the
;; reentrant lock-word scheme above. `Semaphore`'s one field (`permits`, at `this + 0`) is the
;; permit count itself: `acquire` spins a CAS loop decrementing it once it observes a positive
;; count; `release` is a plain atomic increment (no thread ever needs to wait for a `release`).
(func $shared_semaphore_acquire (param $sem i32)
    (local $cur i32)
    (block $done
        (loop $retry
            local.get $sem
            i32.atomic.load
            local.set $cur
            local.get $cur
            i32.const 0
            i32.gt_s
            (if
                (then
                    local.get $sem
                    local.get $cur
                    local.get $cur
                    i32.const 1
                    i32.sub
                    i32.atomic.rmw.cmpxchg
                    local.get $cur
                    i32.eq
                    br_if $done
                )
            )
            br $retry
        )
    )
)
(func $shared_semaphore_release (param $sem i32)
    local.get $sem
    i32.const 1
    i32.atomic.rmw.add
    drop
    local.get $sem
    i32.const 1
    memory.atomic.notify
    drop
)
(func $shared_semaphore_try_acquire (param $sem i32) (result i32)
    (local $cur i32)
    local.get $sem
    i32.atomic.load
    local.set $cur
    local.get $cur
    i32.const 0
    i32.gt_s
    (if (result i32)
        (then
            local.get $sem
            local.get $cur
            local.get $cur
            i32.const 1
            i32.sub
            i32.atomic.rmw.cmpxchg
            local.get $cur
            i32.eq
        )
        (else
            i32.const 0
        )
    )
)
(func $shared_semaphore_try_acquire_for (param $sem i32) (param $timeout_ms i32) (result i32)
    (local $cur i32)
    (local $timeout_ns i64)
    (local $wait_res i32)
    (local $ok i32)
    local.get $timeout_ms
    i32.const 0
    i32.le_s
    (if
        (then
            local.get $sem
            call $shared_semaphore_try_acquire
            return
        )
    )
    local.get $timeout_ms
    i64.extend_i32_s
    i64.const 1000000
    i64.mul
    local.set $timeout_ns
    (block $done (result i32)
        (loop $retry
            local.get $sem
            call $shared_semaphore_try_acquire
            local.set $ok
            local.get $ok
            (if
                (then
                    local.get $ok
                    br $done
                )
            )
            local.get $sem
            i32.atomic.load
            local.set $cur
            local.get $sem
            local.get $cur
            local.get $timeout_ns
            memory.atomic.wait32
            local.set $wait_res
            local.get $wait_res
            i32.const 2
            i32.eq
            (if
                (then
                    local.get $sem
                    call $shared_semaphore_try_acquire
                    br $done
                )
            )
            br $retry
        )
        i32.const 0
    )
)

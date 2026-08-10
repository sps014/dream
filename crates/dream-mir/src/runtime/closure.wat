;; --- fun(...) value ABI: a boxed 2-word `[funcidx][env]` heap block --------------------------
;;
;; Every `fun(...)`-typed value (a bare function reference or a lambda, capturing or not) is this
;; box, not a raw table index: `env` is 0 for anything non-capturing, or a capturing lambda's
;; environment object (an `object`-typed pointer, reinterpreted as `i32`) otherwise. An indirect
;; call through a `fun(...)` value unboxes it: `$__closure_env` is set from the box's `env` word
;; (read by the lifted callee's prologue, which reinterprets it back to its own environment
;; struct type — see `analyze_lambda`/`hir_set_indirect_call_through_box`), then the call proceeds
;; through the box's `funcidx` word via the ordinary `call_indirect`. This keeps `call_indirect`'s
;; signature completely unchanged from the pre-closures ABI.
;;
;; Funcboxes are ordinary ARC heap values (`TyKind::Func` is a reference). `$funcbox_new` retains
;; a non-null env so the box owns it; `$release_funcbox` releases that env when the last fun
;; reference drops, reclaiming CaptureCell / env-array storage with the closure.
(func $funcbox_new (param $funcidx i32) (param $env i32) (result i32)
    (local $box i32)
    i32.const 8
    i32.const 0
    call $malloc
    local.set $box
    local.get $box
    local.get $funcidx
    i32.store
    local.get $box
    i32.const 4
    i32.add
    local.get $env
    i32.store
    ;; Own a retain on the environment so it outlives the creating function's scope-exit release.
    (block $skip_retain
        local.get $env
        i32.eqz
        br_if $skip_retain
        local.get $env
        call $retain
    )
    local.get $box
)

(func $funcbox_funcidx (param $box i32) (result i32)
    local.get $box
    i32.load
)

(func $funcbox_env (param $box i32) (result i32)
    local.get $box
    i32.const 4
    i32.add
    i32.load
)

;; Deep-release a `fun(...)` value: drop the box's RC; when it hits zero, release the env (if any)
;; then free the box. Typed `$release_funcbox` is required because malloc tag 0 does not deep-free
;; through `$release_object` / `$release_generic`.
(func $release_funcbox (param $ptr i32)
    (local $rc i32)
    (local $nc i32)
    (local $env i32)
    local.get $ptr
    i32.eqz
    (if (then (return)))
    local.get $ptr
    i32.const 4
    i32.sub
    local.set $rc
    local.get $rc
    i32.load
    i32.const 1
    i32.sub
    local.set $nc
    local.get $rc
    local.get $nc
    i32.store
    local.get $nc
    i32.eqz
    (if (then
        local.get $ptr
        i32.const 4
        i32.add
        i32.load
        local.set $env
        (block $skip_env
            local.get $env
            i32.eqz
            br_if $skip_env
            local.get $env
            call $release_object
        )
        local.get $ptr
        call $free
    ))
)

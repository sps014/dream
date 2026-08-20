;; Opt-in `defer` enter/leave. Native C owns the destroy queue; WASM last-ref stays immediate
;; so programs without the native runtime still assemble. Depth is tracked for ABI match.
(func $dream_defer_enter
)
(func $dream_defer_leave (param $q i32)
)
(func $dream_defer_drain_all
)

;; `Vector<T>` and autovec emit WASM SIMD (`v128`) inline. This file is kept so the runtime
;; prelude still concatenates a SIMD section; helpers live in the emitter, not as `$simd_*` funcs.

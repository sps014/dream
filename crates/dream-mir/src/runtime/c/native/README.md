# Native (host) C runtime

Not used by `cargo test` / WAT splice. Companion to wasm `../include/dream_rt.h`.

- Pointers are `uintptr_t`. Do not truncate with `(uint32_t)`.
- Copies use `memcpy`.
- Heap is size-class freelists + `mmap` / `VirtualAlloc` (`heap.c`).
- `retain`/`release` are `always_inline` in [`include/dream_rt_native.h`](include/dream_rt_native.h).
- SIMD helpers use platform vector width (`DREAM_F32_LANES` is 8 on AVX2).
- Pike epsilon-closure uses computed goto (`pike.c`).
- Guest helpers: `strings.c`, `object.c`, `format.c`, `panic.c`, `weak.c`, `closure.c`, `async.c`, `sync.c`.
- Host print/math/file stubs: `host.c`. Linked by `dream run --backend c` and `tests/e2e_native_c.rs`.

```bash
cc -O3 -flto -march=native -o /tmp/dream-rt-bench \
  crates/dream-mir/src/runtime/c/native/heap.c \
  crates/dream-mir/src/runtime/c/native/pike.c \
  crates/dream-mir/src/runtime/c/native/bench_hotpath.c
/tmp/dream-rt-bench
```

# Native (host) C runtime

Linked into native programs and golden tests. Shared helpers also serve the wasm runtime.

- Native references are byte pointers (`dream_ptr`), not integer handles. Wasm keeps linear-memory offsets.
- Async completion's `dream_result` transports scalar bits or pointer-width addresses in a 64-bit slot; reference conversion is restricted to that transport boundary.
- Copies use `memcpy`.
- Heap is size-class freelists + `mmap` / `VirtualAlloc` (`heap.c`).
- `retain`/`release` are `always_inline` in [`include/dream_rt_native.h`](include/dream_rt_native.h).
- SIMD helpers use platform vector width (`DREAM_F32_LANES` is 8 on AVX2).
- Regex is vendored PCRE2-16 with JIT (`../regex.c` `-DDREAM_NATIVE` + [`../pcre2/README.md`](../pcre2/README.md)), linked only when the program uses `Regex`.
- Core runtime: `heap.c`, `strings.c`, `object.c`, `format.c`, `panic.c`, `weak.c`, `closure.c`, `async.c`, `sync.c`.
- Native host operations: `fs.c`, `file_handle.c`, `dirs.c`, `process.c`, `env.c`,
  `time.c`, `stdio.c` and `math.c`. Shared string conversion and sorted-name helpers
  live in `host_support.c`, with private declarations in `include/dream_host_support.h`.
  These units are linked by `dream run` and golden e2e through `NATIVE_CORE_C`.
- Compiled to bitcode by the pinned clang (`dreamer toolchain install llvm`); the system `cc` (or Zig via `dreamer toolchain install cc`) only links.

```bash
cc -O3 -flto -march=native -o /tmp/dream-rt-bench \
  crates/dream-mir/src/runtime/c/native/heap.c \
  crates/dream-mir/src/runtime/c/native/strings.c \
  crates/dream-mir/src/runtime/c/native/bench_hotpath.c
/tmp/dream-rt-bench
```

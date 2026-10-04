# Layered C runtime

- `core/`: portable heap/ARC, regions, strings, formatting, weak references, deferred drops,
  closures, FFI conversion, panic and memory primitives. `core/include/dream_core.h` defines
  the target-shaped ABI; `include/dream_abi.h` supplies shared numeric constants.
- `sys/native/`: POSIX/Win32 adapters using `dream_thread.h`, mapped pages, stdio, files,
  environment, clocks, callbacks and scheduling. Native OS choices stay here.
- `sys/wasi/`: wasm linear-memory heap representation, JS/WASI adapters, globals, allocation
  bridge and synchronization. Shared core logic is compiled once for each target ABI.
- `sys/shared/`: scheduling compiled for native and wasm32, with target-specific wakeup services.
- Host capabilities (`net`, `gpu`, `webview`) remain separate `dream-host-*` crates, selected
  by the existing capability registry. Regex remains the optional vendored PCRE2 module.

`../modules.rs` lists shared core and sys units by layer. LLVM runtime caches fingerprint
sources and headers from all layers. Guest code remains C only.

`include/dream_platform.h` defines the embedding platform table. Install it through
`dream_set_platform` before any runtime use. Keep the table and allocation domain valid for
its entire lifetime; replacing a live allocator is invalid. Every callback is required.
Mapped regions are zeroed and aligned to 16 bytes. Heap and weak locks are distinct domains;
weak operations may take the heap lock. `object_drop` retires sys synchronization state.
Writes receive UTF-8 bytes or native-endian UTF-16 units and a stream number; abort must terminate without returning.
Native defaults use libc allocation, OS maps and the existing POSIX/Win32 lock abstraction.
Wasm defaults use the guest heap and the JS host text writer. Panic output needs no guest allocation.

Panics use fixed storage, including allocator exhaustion while a lock is held. Default
output streams the complete message; panic hooks receive a bounded UTF-8 copy. There is no
stdio constructor, so embedding does not change the host application's buffering policy.

Run `python scripts/check_freestanding.py` with clang and lld. It compiles every core unit
with `-ffreestanding -nostdlib -nostdinc`, using only clang's builtin headers and declaration
adapters for vendor headers, then links all functions without section collection, a target
SDK, CRT, sys objects or libc. The anchor supplies only program-generated glue and the
embedding platform symbol. The same check runs in CI. Runtime behavior and deterministic
allocation failures are covered by `tests/runtime_platform.rs` on native hosts. The gate
also rejects first-party C units over 600 lines (vendored PCRE2/SLJIT are excluded).
`tests/runtime_wasi_platform.rs` bounds imported memory to verify page-growth failure reports
through the WASI text/abort callbacks without allocating or growing memory.

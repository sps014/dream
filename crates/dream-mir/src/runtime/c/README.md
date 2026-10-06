# Runtime layers and embedding

Use this guide when changing the C runtime or embedding it in another application. Each layer has one responsibility; platform services stay outside portable core code.

## Layers

- `core/` manages allocation, reference counts, regions, strings, formatting, weak references, deferred cleanup, closures, conversions, and panics.
- `sys/native/` adapts operating-system services, including pages, locks, files, environment variables, clocks, callbacks, and scheduling.
- `sys/wasi/` adapts WebAssembly memory, globals, synchronization, and browser/Node services.
- `sys/shared/` provides scheduling common to both target families.

The host capability crates provide Unicode, crypto, process, and timezone services separately. PCRE2 remains the vendored regular-expression module. `../modules.rs` registers runtime units; cache fingerprints include sources and headers from every layer.

## Embed the runtime

`include/dream_platform.h` defines the platform callback table. Install it with `dream_set_platform` before using the runtime. Every callback is required. Keep the table and allocation domain alive for the entire runtime lifetime; do not replace a live allocator.

Mapped regions must be zeroed and aligned to 16 bytes. Heap locks and weak-reference locks are separate; weak operations may take the heap lock. `object_drop` retires system synchronization state.

Output callbacks receive UTF-8 bytes or native-endian UTF-16 units and a stream number. The abort callback must terminate without returning. Native defaults use system allocation and locks; WebAssembly defaults use the guest heap and host text writer.

## Handle allocation failures

Panic reporting uses fixed storage so it can still report allocation exhaustion while a lock is held. Normal output writes the complete message; panic hooks receive a bounded UTF-8 copy. There is no stdio constructor, so embedding does not change the host's buffering policy.

## Check a runtime change

Run `python scripts/check_freestanding.py` from the repository root with Clang and lld available. It builds every core unit without a system SDK or standard library and links all functions. It supplies only generated glue and the embedding platform symbol. First-party C units over 600 lines fail this check; vendored PCRE2 and SLJIT are excluded.

`tests/runtime_platform.rs` checks native behavior and deterministic allocation failures. `tests/runtime_wasi_platform.rs` checks bounded-memory growth failure and allocation-free reporting through the WebAssembly platform callbacks.

# sljit (PCRE2 JIT)

Stack-less JIT used **only** by native PCRE2 (`SUPPORT_JIT` when `PCRE2_WASM` is unset).
WASM regex is the PCRE2 interpreter; it does not compile these files.

Sources live under `sljit_src/` (including `allocator_src/`). They come from the PCRE2
10.45 `deps/sljit` tree.

Refresh together with [`../../pcre2/README.md`](../../pcre2/README.md): copy upstream
`deps/sljit/sljit_src/` over this `sljit_src/`. Native `regex.c` / PCRE2 JIT compile
include these via PCRE2’s usual `SLJIT_DIR` layout.

Do not enable executable-memory allocators that assume W^X inside WASM linear memory.

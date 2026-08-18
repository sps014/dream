# Guest WASM runtime

Dream splices `*.wat` in this directory into the **same module** as user code (`include_str!` in `dream-mir`). That is what `dream` / `cargo test` / Windows CI use. They never run clang.

**Same-module helpers** (`allocator.wat`, `strings.wat`, `object.wat`, `format.wat`, `panic.wat`, `weak.wat`, `closure.wat`, `sync.wat`, `async.wat`, `alloc_lock.wat`, `simd.wat`) are **authored WAT**. Edit those files.

**Linked libraries** (PCRE2 regex) are too large for that. C under [`c/regex.c`](c/regex.c) + [`c/pcre2/`](c/pcre2/) is compiled by `scripts/build-runtime.sh` (wasi-sdk 33) to [`regex.wat`](regex.wat). Do not hand-edit `regex.wat`.

Native `--native-c` is a **separate** ABI under [`c/native/`](c/native/) (`uintptr_t`, `memcpy`, mmap heap). It does not go through WAT.

`--release` still runs fused `wasm-opt -O3` on the whole guest module.

See [`c/README.md`](c/README.md) for C (regex + native).

## Rebuild `regex.wat` (macOS / Linux)

`cargo build` / `dream` do **not** need this. Only run it when you change `c/regex.c`, `c/regex_wasm_libc.c`, or vendored PCRE2.

**Do not use Apple/Xcode `clang`.** Pin **wasi-sdk 33** from [wasi-sdk releases](https://github.com/WebAssembly/wasi-sdk/releases/tag/wasi-sdk-33). You also need `wasm2wat` (WABT) or `wasm-tools print` on `PATH`.

### 1. Install wasi-sdk 33

| Host | Asset |
|---|---|
| macOS Apple Silicon | `wasi-sdk-33.0-arm64-macos.tar.gz` |
| macOS Intel | `wasi-sdk-33.0-x86_64-macos.tar.gz` |
| Linux aarch64 | `wasi-sdk-33.0-arm64-linux.tar.gz` |
| Linux x86_64 | `wasi-sdk-33.0-x86_64-linux.tar.gz` |

```bash
# Example: macOS arm64. Swap the URL/filename for Linux or Intel Mac (table above).
mkdir -p ~/.dream/toolchains
curl -L --fail -o /tmp/wasi-sdk.tar.gz \
  https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-33/wasi-sdk-33.0-arm64-macos.tar.gz
tar -xzf /tmp/wasi-sdk.tar.gz -C ~/.dream/toolchains
export WASI_SDK_PATH="$HOME/.dream/toolchains/wasi-sdk-33.0-arm64-macos"
```

`scripts/build-runtime.sh` also finds `~/.dream/toolchains/wasi-sdk-*/bin/clang` if `WASI_SDK_PATH` is unset.

### 2. Install `wasm2wat` (if you do not already have `wasm-tools`)

```bash
brew install wabt   # macOS
```

Linux: `wabt` package or a WABT release on `PATH`.

### 3. Link PCRE2 → `regex.wat`

```bash
export WASI_SDK_PATH="$HOME/.dream/toolchains/wasi-sdk-33.0-arm64-macos"
scripts/build-runtime.sh
scripts/build-runtime.sh --check
```

Apple clang without wasm32: `--check` **skips**. A machine that has wasi-sdk must pass `--check`.

**Windows:** do not install wasi-sdk on `windows-latest` CI. Shipping `dream.exe` embeds the checked-in `.wat` files.

`TAG_*` / heap offsets / `DREAM_REGEX_*` live in [`c/include/dream_abi.h`](c/include/dream_abi.h) and [`../abi.rs`](../abi.rs) (lockstep test `dream_abi_h_matches_abi_rs`).

Interned `""` / `"true"` / `"false"` / `"-"` are emitter globals `$__rt_str_empty` / `_true` / `_false` / `_minus`.

## Native C (host clang, not WAT)

[`c/native/`](c/native/) : `uintptr_t` pointers, `memcpy`, mmap size-class heap, platform SIMD width. Linked C libraries (PCRE2) come from the catalog only when `RuntimeNeed` is set. Default `dream run` stays wasmtime. See [`c/native/README.md`](c/native/README.md).

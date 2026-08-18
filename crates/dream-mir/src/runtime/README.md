# Guest WASM runtime

Dream splices `*.wat` in this directory into the **same module** as user code (`include_str!` in `dream-mir`). That is what `dream` / `cargo test` / Windows CI use. They never run clang.

C under [`c/`](c/) is the **authoring** source for the same helpers. `scripts/build-runtime.sh` compiles it to wasm32 and extracts WAT. Until a generated file is copied over after extract gates + golden checks, **handwritten `*.wat` remains the emit artifact** (hybrid). `--release` still runs fused `wasm-opt -O3` on the whole module.

See [`c/README.md`](c/README.md) for C do/don't.

## Build on macOS and Linux

`cargo build` / `dream` do **not** need this. Only run it when you change `c/*.c` or `c/include/*.h`.

**Do not use Apple/Xcode `clang`.** It has no `wasm32` target. Pin **wasi-sdk 33** from [wasi-sdk releases](https://github.com/WebAssembly/wasi-sdk/releases/tag/wasi-sdk-33).

You also need `wasm2wat` (from [WABT](https://github.com/WebAssembly/wabt/releases)) or `wasm-tools print` on `PATH` so the script can dump relocatable `.o` files to WAT.

### 1. Install wasi-sdk 33

Pick the tarball for your OS and CPU:

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

On Linux x86_64 the last two lines are:

```bash
tar -xzf /tmp/wasi-sdk.tar.gz -C ~/.dream/toolchains
export WASI_SDK_PATH="$HOME/.dream/toolchains/wasi-sdk-33.0-x86_64-linux"
```

`scripts/build-runtime.sh` also finds `~/.dream/toolchains/wasi-sdk-*/bin/clang` if `WASI_SDK_PATH` is unset.

### 2. Install `wasm2wat` (if you do not already have `wasm-tools`)

**macOS (Homebrew):**

```bash
brew install wabt
```

**Linux:** install the `wabt` package (`sudo apt install wabt` on Debian/Ubuntu) or unpack a WABT release and put `wasm2wat` on `PATH`.

### 3. Compile C → `c/generated/*.wat`

From the repo root:

```bash
export WASI_SDK_PATH="$HOME/.dream/toolchains/wasi-sdk-33.0-arm64-macos"  # or the linux dir from step 1
scripts/build-runtime.sh
scripts/build-runtime.sh --check   # same compile; exit 1 if extract gates fail
```

Apple clang without wasm32: `--check` **skips** (so `cargo test` stays green). A machine that has wasi-sdk must pass `--check`.

### 4. Promote generated WAT (optional)

If extract gates pass **and** `--release` goldens are not slower, copy `c/generated/<file>.wat` over `<file>.wat`. Allocator debug/thread placeholders (`;;@DEBUG_*@` / `;;@ALLOC_LOCK_*@`) stay handwritten until C variants exist.

**Windows:** do not install wasi-sdk on `windows-latest` CI. Shipping `dream.exe` embeds the checked-in `.wat` files.

`TAG_*` / heap offsets live in [`c/include/dream_abi.h`](c/include/dream_abi.h) and [`../abi.rs`](../abi.rs) (lockstep test `dream_abi_h_matches_abi_rs`).

Interned `""` / `"true"` / `"false"` / `"-"` are emitter globals `$__rt_str_empty` / `_true` / `_false` / `_minus`, not baked addresses.

## Native C (host clang, not WAT)

[`c/native/`](c/native/) is a **separate** ABI: `uintptr_t` pointers, `memcpy`, mmap size-class heap, platform SIMD width, Pike computed goto. It is not spliced into WASM. Default `dream run` stays wasmtime until `scripts/bench-native-c.sh` plus `microbenches.dream` beat `--release` wasm. See [`c/native/README.md`](c/native/README.md).

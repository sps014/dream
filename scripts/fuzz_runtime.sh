#!/usr/bin/env bash
# libFuzzer + ASan/UBSan over the portable runtime's text paths (tests/fuzz/runtime_fuzz.c).
# Usage: scripts/fuzz_runtime.sh [seconds]   (default 30; uses the pinned clang)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SECONDS_BUDGET="${1:-30}"
LLVM_BIN="${DREAM_LLVM:-$HOME/.dream/toolchains/llvm-22.1.8/bin}"
CLANG="$LLVM_BIN/clang"
[ -x "$CLANG" ] || CLANG=clang
RT="$ROOT/crates/dream-mir/src/runtime/c"
OUT="$ROOT/target/fuzz"
mkdir -p "$OUT/corpus"
SYSROOT=()
if [ "$(uname)" = Darwin ]; then
  SYSROOT=(-isysroot "$(xcrun --show-sdk-path)")
fi

"$CLANG" "${SYSROOT[@]}" -std=gnu11 -O1 -g -pthread -fsanitize=fuzzer,address,undefined -fno-sanitize-recover=all \
  -I"$RT/core/include" -I"$RT/sys/native/include" -I"$RT/include" \
  "$RT/core/platform.c" "$RT/core/utf8.c" "$RT/sys/native/platform.c" \
  "$RT/core/heap.c" "$RT/core/ownership.c" "$RT/core/heap_maps.c" "$RT/core/publish.c" \
  "$RT/core/region.c" "$RT/core/weak.c" "$RT/sys/native/sync.c" "$RT/core/strings.c" \
  "$RT/core/string_view.c" "$RT/core/format.c" "$RT/core/object.c" \
  "$ROOT/tests/fuzz/runtime_fuzz.c" -o "$OUT/runtime_fuzz"

"$OUT/runtime_fuzz" -max_total_time="$SECONDS_BUDGET" -max_len=4096 "$OUT/corpus"

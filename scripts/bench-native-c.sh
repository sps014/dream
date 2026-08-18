#!/usr/bin/env bash
# Compile the native C hotpath (uintptr + memcpy + mmap). Does not change `dream run` (wasmtime).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CC="${CC:-cc}"
OUT="${OUT:-/tmp/dream-rt-bench}"
NATIVE="$ROOT/crates/dream-mir/src/runtime/c/native"
"$CC" -O3 -flto -march=native -o "$OUT" \
  "$NATIVE/heap.c" "$NATIVE/weak.c" "$NATIVE/pike.c" "$NATIVE/bench_hotpath.c"
echo "== native C runtime hotpath ($OUT) =="
"$OUT"
echo "(compare to wasmtime via ./scripts/run-microbenches.sh; do not switch default dream run until these beat --release wasm)"

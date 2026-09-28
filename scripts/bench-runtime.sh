#!/usr/bin/env bash
# Microbenchmark the C runtime hotpath (heap, weak refs, strings) outside the compiler.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CC="${CC:-cc}"
OUT="${OUT:-/tmp/dream-rt-bench}"
NATIVE="$ROOT/crates/dream-mir/src/runtime/c/native"
"$CC" -O3 -flto -march=native -o "$OUT" \
  "$NATIVE/heap.c" "$NATIVE/weak.c" "$NATIVE/strings.c" "$NATIVE/async.c" "$NATIVE/bench_hotpath.c"
echo "== C runtime hotpath ($OUT) =="
"$OUT"
echo "(language-level benches: ./scripts/run-microbenches.sh)"

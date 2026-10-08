#!/usr/bin/env bash
# Microbenchmark the C runtime hotpath (heap, weak refs, strings) outside the compiler.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CC="${CC:-cc}"
OUT="${OUT:-/tmp/dream-rt-bench}"
NATIVE="$ROOT/crates/dream-mir/src/runtime/c/sys/native"
CORE="$ROOT/crates/dream-mir/src/runtime/c/core"
"$CC" -I"$CORE/include" -I"$NATIVE/include" -I"$CORE/../include" -O3 -flto -march=native -o "$OUT" \
  "$CORE/heap.c" "$CORE/ownership.c" "$CORE/heap_maps.c" "$CORE/publish.c" "$CORE/region.c" "$CORE/weak.c" "$NATIVE/sync.c" "$CORE/strings.c" "$NATIVE/../shared/async.c" "$CORE/panic.c" "$NATIVE/bench_hotpath.c" "$CORE/platform.c" "$CORE/utf8.c" "$NATIVE/platform.c"
echo "== C runtime hotpath ($OUT) =="
"$OUT"
echo "(language-level benches: ./scripts/run-microbenches.sh)"

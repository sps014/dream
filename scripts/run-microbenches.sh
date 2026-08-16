#!/usr/bin/env bash
# Run Dream ARC/stdlib microbenches on the LLVM native backend.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BENCH="$ROOT/tests/bench/microbenches.dream"
DREAM="${DREAM:-$ROOT/target/release/dream}"
if [[ ! -x "$DREAM" ]]; then
  DREAM="$ROOT/target/debug/dream"
fi
if [[ ! -x "$DREAM" ]]; then
  echo "dream binary not found; build with: cargo build --release" >&2
  exit 1
fi

OUT_DIR="${OUT_DIR:-$ROOT/tests/bench/out}"
mkdir -p "$OUT_DIR"

echo "== native (LLVM + dream-rt) =="
"$DREAM" --release run "$BENCH" | tee "$OUT_DIR/native.txt"

echo "Wrote results under $OUT_DIR"
echo "Compare ns_per_op to tests/bench/BASELINE.md (wasmtime-era numbers; native LLVM should be faster on CPU benches)."

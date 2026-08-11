#!/usr/bin/env bash
# Run Dream ARC/stdlib microbenches on native wasmtime and optionally via Node web runtime.
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

echo "== native (wasmtime) =="
"$DREAM" --release run "$BENCH" | tee "$OUT_DIR/native.txt"

if command -v node >/dev/null 2>&1; then
  echo "== browser/node runtime =="
  WAT_OUT="$OUT_DIR/microbenches.wat"
  "$DREAM" --release --runtime --node --target node "$BENCH" -o "$WAT_OUT" 2>/dev/null \
    || "$DREAM" --release --runtime --node "$BENCH" 2>/dev/null || true
  # Fallback: compile next to source then copy artifacts
  if [[ ! -f "${BENCH%.dream}.node.runtime.js" && ! -f "$OUT_DIR/microbenches.node.runtime.js" ]]; then
    "$DREAM" --release --runtime --node "$BENCH" || true
  fi
  RUNTIME_JS="$(ls -1 "${BENCH%.dream}".node.runtime.js 2>/dev/null | head -1 || true)"
  WASM="$(ls -1 "${BENCH%.dream}".wasm 2>/dev/null | head -1 || true)"
  if [[ -n "${RUNTIME_JS:-}" && -f "$RUNTIME_JS" && -n "${WASM:-}" ]]; then
    cp "$RUNTIME_JS" "$OUT_DIR/" 2>/dev/null || true
    cp "$WASM" "$OUT_DIR/" 2>/dev/null || true
    cp "${BENCH%.dream}.abi.json" "$OUT_DIR/" 2>/dev/null || true
    node -e "
      const fs = require('fs');
      const path = require('path');
      const runtime = process.argv[1];
      // Many Dream node runtimes export a run/main helper; if not, load and rely on side effects.
      require(path.resolve(runtime));
    " "$RUNTIME_JS" 2>/dev/null | tee "$OUT_DIR/node.txt" || {
      echo "(node runtime smoke skipped — invoke via dream run for timing)" | tee "$OUT_DIR/node.txt"
    }
  else
    echo "(node runtime artifacts not produced; native results recorded)" | tee "$OUT_DIR/node.txt"
  fi
else
  echo "(node not available; skipping browser/node target)" | tee "$OUT_DIR/node.txt"
fi

echo "Wrote results under $OUT_DIR"

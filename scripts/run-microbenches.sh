#!/usr/bin/env bash
# Run Dream ARC/stdlib microbenches on native wasmtime and optionally compare to C# Release.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BENCH="$ROOT/tests/bench/microbenches.dream"
CSHARP_DIR="$ROOT/tests/bench/csharp"
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

COMPARE=0
if command -v dotnet >/dev/null 2>&1 && [[ -f "$CSHARP_DIR/DreamBench.csproj" ]]; then
  echo "== csharp (dotnet -c Release) =="
  (cd "$CSHARP_DIR" && dotnet restore -q \
    && DREAM_SCORES="$OUT_DIR/native.txt" dotnet run -c Release --no-restore \
      2>"$OUT_DIR/csharp.compare.txt" | tee "$OUT_DIR/csharp.txt")
  COMPARE=1
else
  echo "(dotnet / tests/bench/csharp not available; skipping C# compare)" | tee "$OUT_DIR/csharp.txt"
fi

if [[ "$COMPARE" -eq 1 ]]; then
  python3 - "$OUT_DIR/native.txt" "$OUT_DIR/csharp.txt" <<'PY' | tee "$OUT_DIR/compare.txt"
import sys
from pathlib import Path

def load(path):
    out = {}
    for line in Path(path).read_text().splitlines():
        if not line.startswith("bench "):
            continue
        # Dream: bench name ns_total=… iters=… ns_per_op=N
        # C#:    bench name ns_per_op=N
        rest = line[len("bench "):]
        name, _, tail = rest.partition(" ")
        key = "ns_per_op="
        idx = tail.rfind(key) if key in tail else -1
        if idx < 0 and key in rest:
            # name may be the only token before ns_per_op when format is compact
            idx = rest.rfind(key)
            name = rest[:idx].strip()
            num = rest[idx + len(key):].strip()
        else:
            num = tail[idx + len(key):].strip() if idx >= 0 else ""
        try:
            out[name] = int(num)
        except ValueError:
            pass
    return out

dream = load(sys.argv[1])
csharp = load(sys.argv[2])
names = list(dict.fromkeys([*dream.keys(), *csharp.keys()]))
print(f"{'bench':<18} {'Dream':>8} {'C#':>8} {'C#/Dream':>10}  note")
print("-" * 64)
print("Dream = Wasm/wasmtime + ARC; C# = native JIT + GC. Ratios are not ARC-only.")
for name in names:
    d = dream.get(name)
    c = csharp.get(name)
    if d is None or c is None:
        print(f"{name:<18} {d or '-':>8} {c or '-':>8} {'n/a':>10}")
        continue
    ratio = c / d if d else float("inf")
    if ratio < 1:
        note = f"C# {1/ratio:.1f}x faster"
    else:
        note = f"C# {ratio:.1f}x slower"
    print(f"{name:<18} {d:8d} {c:8d} {ratio:10.2f}  {note}")
PY
fi

if command -v node >/dev/null 2>&1; then
  echo "== browser/node runtime =="
  WAT_OUT="$OUT_DIR/microbenches.wat"
  "$DREAM" --release --runtime --node --target node "$BENCH" -o "$WAT_OUT" 2>/dev/null \
    || "$DREAM" --release --runtime --node "$BENCH" 2>/dev/null || true
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
      const path = require('path');
      require(path.resolve(process.argv[1]));
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

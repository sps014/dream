#!/usr/bin/env bash
# Run Dream ARC/stdlib microbenches: wasmtime, native C (--backend c), optional C# Release.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BENCH="$ROOT/tests/bench/microbenches.dream"
CSHARP_DIR="$ROOT/tests/bench/csharp"
DREAM="${DREAM:-$ROOT/target/release/dream}"
if [[ ! -x "$DREAM" ]]; then
  DREAM="$ROOT/target/debug/dream"
fi
if [[ ! -x "$DREAM" ]]; then
  echo "dream binary not found; build with: cargo build --release --features native" >&2
  exit 1
fi

OUT_DIR="${OUT_DIR:-$ROOT/tests/bench/out}"
mkdir -p "$OUT_DIR"

echo "== wasmtime (Dream --release) =="
"$DREAM" --release run "$BENCH" | tee "$OUT_DIR/native.txt"

echo "== native C (Dream --release --backend c) =="
"$DREAM" --release --native-c run "$BENCH" | tee "$OUT_DIR/native_c.txt"

if command -v dotnet >/dev/null 2>&1 && [[ -f "$CSHARP_DIR/DreamBench.csproj" ]]; then
  echo "== csharp (dotnet -c Release) =="
  (cd "$CSHARP_DIR" && dotnet restore -q \
    && DREAM_SCORES="$OUT_DIR/native_c.txt" dotnet run -c Release --no-restore \
      2>"$OUT_DIR/csharp.compare.txt" | tee "$OUT_DIR/csharp.txt")
else
  echo "(dotnet / tests/bench/csharp not available; skipping C# compare)" | tee "$OUT_DIR/csharp.txt"
fi

python3 - "$OUT_DIR/native.txt" "$OUT_DIR/native_c.txt" "$OUT_DIR/csharp.txt" <<'PY' | tee "$OUT_DIR/compare.txt"
import sys
from pathlib import Path

def load(path):
    out = {}
    p = Path(path)
    if not p.exists():
        return out
    for line in p.read_text().splitlines():
        if not line.startswith("bench "):
            continue
        rest = line[len("bench "):]
        parts = rest.split()
        if not parts:
            continue
        name = parts[0]
        kv = {}
        for tok in parts[1:]:
            if "=" in tok:
                k, _, v = tok.partition("=")
                kv[k] = v
        try:
            if "ns_total" in kv and "iters" in kv:
                total = float(kv["ns_total"])
                iters = float(kv["iters"])
                out[name] = total / iters if iters else 0.0
            else:
                out[name] = float(kv["ns_per_op"])
        except (KeyError, ValueError):
            pass
    return out

wasm = load(sys.argv[1])
native_c = load(sys.argv[2])
csharp = load(sys.argv[3]) if len(sys.argv) > 3 else {}
names = list(dict.fromkeys([*wasm.keys(), *native_c.keys(), *csharp.keys()]))
print(f"{'bench':<18} {'Wasm':>8} {'C':>8} {'C#':>8}  {'C/Wasm':>8}  {'C#/C':>8}  note")
print("-" * 88)
print("Wasm = wasmtime + ARC; C = cc -O3 -flto; C# = RyuJIT + GC. ns/op (float).")
for name in names:
    w = wasm.get(name)
    c = native_c.get(name)
    n = csharp.get(name)
    def fmt(v):
        return f"{v:8.1f}" if v is not None else f"{'-':>8}"
    def ratio(a, b):
        if a is None or b is None or a == 0:
            return "     n/a"
        return f"{b/a:8.2f}"
    note = ""
    if c is not None and w is not None and c != 0:
        r = w / c
        note = f"C {r:.1f}x vs Wasm" if r >= 1 else f"Wasm {1/r:.1f}x vs C"
        if n is not None and c != 0:
            r2 = n / c
            if r2 < 1:
                note += f"; C# {1/r2:.1f}x vs C"
            else:
                note += f"; C {r2:.1f}x vs C#"
    print(f"{name:<18} {fmt(w)} {fmt(c)} {fmt(n)}  {ratio(w, c)}  {ratio(c, n)}  {note}")
PY

echo "Wrote results under $OUT_DIR"

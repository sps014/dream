#!/usr/bin/env bash
# Run Dream ARC/stdlib microbenches (`dream --release run`) and optional C# Release.
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

echo "== native C (Dream --release) =="
"$DREAM" --release run "$BENCH" | tee "$OUT_DIR/native.txt"

if command -v dotnet >/dev/null 2>&1 && [[ -f "$CSHARP_DIR/DreamBench.csproj" ]]; then
  echo "== csharp (dotnet -c Release) =="
  (cd "$CSHARP_DIR" && dotnet restore -q \
    && DREAM_SCORES="$OUT_DIR/native.txt" dotnet run -c Release --no-restore \
      2>"$OUT_DIR/csharp.compare.txt" | tee "$OUT_DIR/csharp.txt")
else
  echo "(dotnet / tests/bench/csharp not available; skipping C# compare)" | tee "$OUT_DIR/csharp.txt"
fi

# ---- wasm32 code-size tracking (same suite, three Binaryen levels) ----
echo "== wasm32 code sizes (--release) =="
WASM_DIR="$OUT_DIR/wasm"
mkdir -p "$WASM_DIR"
: > "$OUT_DIR/wasm_sizes.txt"
for LEVEL in 3 s z; do
  STEM="$WASM_DIR/microbenches_O$LEVEL"
  if "$DREAM" --wasm --release "-O$LEVEL" -o "$STEM" "$BENCH" >&2; then
    RAW=$(stat -f%z "$STEM.wasm" 2>/dev/null || stat -c%s "$STEM.wasm")
    GZ=$(stat -f%z "$STEM.wasm.gz" 2>/dev/null || stat -c%s "$STEM.wasm.gz" 2>/dev/null || echo 0)
    BR=$(stat -f%z "$STEM.wasm.br" 2>/dev/null || stat -c%s "$STEM.wasm.br" 2>/dev/null || echo 0)
    printf 'level=O%s wasm=%s gz=%s br=%s\n' "$LEVEL" "$RAW" "$GZ" "$BR" | tee -a "$OUT_DIR/wasm_sizes.txt"
  else
    echo "level=O$LEVEL compile failed" | tee -a "$OUT_DIR/wasm_sizes.txt"
  fi
done

# ---- optional wasm timing under Node >= 18 (third compare column) ----
if command -v node >/dev/null 2>&1 && [[ $(node -p 'parseInt(process.versions.node)') -ge 18 ]]; then
  echo "== wasm32 (Node, --release -O3) =="
  NODE_STEM="$WASM_DIR/timing"
  if "$DREAM" --wasm --release --runtime --node -o "$NODE_STEM" "$BENCH" >&2; then
    cat > "$NODE_STEM.runner.mjs" <<'RUNNER'
const [, , rtPath, wasmPath] = process.argv;
const rt = await import(new URL(`file://${rtPath}`).href);
await rt.default.run(wasmPath);
RUNNER
    # shellcheck disable=SC2016
    if node "$NODE_STEM.runner.mjs" "$NODE_STEM.node.runtime.js" "$NODE_STEM.wasm" \
        2>"$OUT_DIR/wasm.err.txt" | tee "$OUT_DIR/wasm.txt"; then
      :
    else
      echo "(node wasm run failed; see $OUT_DIR/wasm.err.txt)" | tee "$OUT_DIR/wasm.txt"
    fi
  else
    echo "(wasm+runtime build failed; skipping Node timing)" | tee "$OUT_DIR/wasm.txt"
  fi
else
  echo "(node >= 18 not available; skipping wasm timing)" | tee "$OUT_DIR/wasm.txt"
fi

python3 - "$OUT_DIR/native.txt" "$OUT_DIR/csharp.txt" "$OUT_DIR/wasm.txt" <<'PY' | tee "$OUT_DIR/compare.txt"
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

native_c = load(sys.argv[1])
csharp = load(sys.argv[2]) if len(sys.argv) > 2 else {}
wasm = load(sys.argv[3]) if len(sys.argv) > 3 else {}
names = list(dict.fromkeys([*native_c.keys(), *csharp.keys(), *wasm.keys()]))
print(f"{'bench':<18} {'C':>8} {'wasm':>8} {'C#':>8}  note")
print("-" * 62)
print("C = Dream native cc -O3; wasm = Dream wasm32 under Node; C# = RyuJIT + GC.")
for name in names:
    c = native_c.get(name)
    w = wasm.get(name)
    n = csharp.get(name)
    def fmt(v):
        return f"{v:8.1f}" if v is not None else f"{'-':>8}"
    def ratio(a, b):
        if a is None or b is None or a == 0:
            return "     n/a"
        return f"{b/a:8.2f}"
    note_parts = []
    if n is not None and c is not None and c != 0:
        r2 = n / c
        note_parts.append(f"C# {r2:.1f}x vs C" if r2 >= 1 else f"C# {1/r2:.1f}x faster")
    if w is not None and c is not None and c != 0:
        r3 = w / c
        note_parts.append(f"wasm {r3:.1f}x vs C")
    print(f"{name:<18} {fmt(c)} {fmt(w)} {fmt(n)}  {'; '.join(note_parts)}")
PY

echo "Wrote results under $OUT_DIR"

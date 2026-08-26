#!/usr/bin/env bash
# Run Dream ARC/stdlib microbenches (`dream --release run`) and optional C# Release.
#
# Stability: the suite runs REPS times (default 5); the comparator reports the per-bench
# MEDIAN (robust to scheduler/GC outliers) plus MIN (the quiet-machine lower bound) and a
# spread% so unstable rows are visible instead of silently misleading. On macOS the whole
# run is wrapped in `caffeinate` so idle sleep can't truncate a pass.
#
# Usage: REPS=7 ./scripts/run-microbenches.sh
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
REPS="${REPS:-5}"

# Prevent idle sleep during measurement on macOS (no-op elsewhere).
CAFF=()
if command -v caffeinate >/dev/null 2>&1; then
  CAFF=(caffeinate -dimsu)
fi

NATIVE_PIPE='"$0" --release run "$1" 2>/dev/null | grep "^bench " || true'

echo "== native C (Dream --release) x$REPS =="
: > "$OUT_DIR/native.txt"
for i in $(seq 1 "$REPS"); do
  "${CAFF[@]}" bash -c "$NATIVE_PIPE" "$DREAM" "$BENCH" > "$OUT_DIR/native.rep$i.txt" || true
done
python3 - "$OUT_DIR" native "$REPS" <<'PY'
import sys
from pathlib import Path
out_dir = Path(sys.argv[1]); stem = sys.argv[2]; reps = int(sys.argv[3])
def load(p):
    rows = {}
    for line in p.read_text().splitlines():
        if not line.startswith("bench "):
            continue
        kv = dict(tok.split("=", 1) for tok in line.split()[1:] if "=" in tok)
        try:
            v = float(kv["ns_total"]) / float(kv["iters"]) if "iters" in kv else float(kv["ns_per_op"])
        except (KeyError, ValueError):
            continue
        name = line.split()[1]
        rows.setdefault(name, []).append((v, line))
    return rows
per_name = {}
for i in range(1, reps + 1):
    p = out_dir / f"{stem}.rep{i}.txt"
    if not p.exists():
        continue
    for name, vals in load(p).items():
        per_name.setdefault(name, []).extend(vals)
with open(out_dir / f"{stem}.txt", "w") as f:
    for name, vals in sorted(per_name.items()):
        # Median by value; emit that representative's ORIGINAL line so the on-disk format
        # (bench <name> ns_total=… iters=… ns_per_op=…) stays byte-compatible.
        vals.sort(key=lambda x: x[0])
        f.write(vals[len(vals) // 2][1] + "\n")
PY
cat "$OUT_DIR/native.txt"

if command -v dotnet >/dev/null 2>&1 && [[ -f "$CSHARP_DIR/DreamBench.csproj" ]]; then
  echo "== csharp (dotnet -c Release) x$REPS =="
  (cd "$CSHARP_DIR" && dotnet build -c Release --nologo -v q >/dev/null 2>&1)
  : > "$OUT_DIR/csharp.txt"
  for i in $(seq 1 "$REPS"); do
    (cd "$CSHARP_DIR" \
      && DREAM_SCORES="$OUT_DIR/native.txt" "${CAFF[@]}" dotnet run -c Release --no-build \
        2>"$OUT_DIR/csharp.compare.txt" | grep '^bench ' > "$OUT_DIR/csharp.rep$i.txt") || true
  done
  python3 - "$OUT_DIR" csharp "$REPS" <<'PY'
import sys
from pathlib import Path
out_dir = Path(sys.argv[1]); stem = sys.argv[2]; reps = int(sys.argv[3])
per_name = {}
for i in range(1, reps + 1):
    p = out_dir / f"{stem}.rep{i}.txt"
    if not p.exists():
        continue
    for line in p.read_text().splitlines():
        if not line.startswith("bench "):
            continue
        try:
            v = float(line.split("ns_per_op=")[1])
        except (IndexError, ValueError):
            continue
        per_name.setdefault(line.split()[1], []).append((v, line))
with open(out_dir / f"{stem}.txt", "w") as f:
    for name, vals in sorted(per_name.items()):
        vals.sort(key=lambda x: x[0])
        f.write(vals[len(vals) // 2][1] + "\n")
PY
else
  echo "(dotnet / tests/bench/csharp not available; skipping C# compare)" | tee "$OUT_DIR/csharp.txt"
fi

python3 - "$OUT_DIR/native.txt" "$OUT_DIR/csharp.txt" "$REPS" <<'PY' | tee "$OUT_DIR/compare.txt"
import sys, statistics
from pathlib import Path

def load(path):
    out = {}
    p = Path(path)
    if not p.exists():
        return out
    for line in p.read_text().splitlines():
        if not line.startswith("bench "):
            continue
        kv = dict(tok.split("=", 1) for tok in line.split()[1:] if "=" in tok)
        try:
            v = float(kv["ns_total"]) / float(kv["iters"]) if "iters" in kv else float(kv["ns_per_op"])
        except (KeyError, ValueError):
            continue
        out[line.split()[1]] = v
    return out

def spread(stem, reps):
    """name -> (median, min, spread_pct)"""
    samples = {}
    out_dir = Path(sys.argv[1]).parent
    for i in range(1, reps + 1):
        p = out_dir / f"{stem}.rep{i}.txt"
        if not p.exists():
            continue
        for line in p.read_text().splitlines():
            if not line.startswith("bench "):
                continue
            kv = dict(tok.split("=", 1) for tok in line.split()[1:] if "=" in tok)
            try:
                v = float(kv["ns_total"]) / float(kv["iters"]) if "iters" in kv else float(kv["ns_per_op"])
            except (KeyError, ValueError):
                continue
            samples.setdefault(line.split()[1], []).append(v)
    out = {}
    for name, vs in samples.items():
        med = statistics.median(vs)
        sp = (max(vs) - min(vs)) / med * 100 if med else 0.0
        out[name] = (med, min(vs), sp)
    return out

reps = int(sys.argv[3])
native_c = load(sys.argv[1])
csharp = load(sys.argv[2]) if len(sys.argv) > 2 else {}
nat_spread = spread("native", reps)
names = list(dict.fromkeys([*native_c.keys(), *csharp.keys()]))
print(f"{'bench':<18} {'C':>9} {'min':>9} {'spr%':>5} {'C#':>9}  note")
print("-" * 66)
print(f"C = median of {reps} runs (native cc -O3); spr% = (max-min)/median.")
for name in names:
    c = native_c.get(name)
    n = csharp.get(name)
    med, lo, sp = nat_spread.get(name, (None, None, 0.0))
    def fmt(v, w=9):
        return f"{v:{w}.1f}" if v is not None else f"{'-':>{w}}"
    note_parts = []
    if n is not None and c is not None and c != 0:
        r2 = n / c
        note_parts.append(f"C {r2:.1f}x faster" if r2 >= 1 else f"C# {1/r2:.1f}x faster")
    flag = " !" if sp > 15 else ""
    print(f"{name:<18} {fmt(c)} {fmt(lo)} {sp:4.0f}{flag} {fmt(n)}  {'; '.join(note_parts)}")
PY

echo "Wrote results under $OUT_DIR"

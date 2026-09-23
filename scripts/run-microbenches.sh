#!/usr/bin/env bash
# Run Dream ARC/stdlib microbenches (`dream --release run`) and optional C# Release.
#
# Stability: the suite runs REPS times (default 5); the comparator reports the per-bench
# MEDIAN (robust to scheduler/GC outliers) and a spread% so unstable rows are visible.
# On macOS the whole run is wrapped in `caffeinate` so idle sleep can't truncate a pass.
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

echo "== Dream (--release) x$REPS =="
: > "$OUT_DIR/native.txt"
for i in $(seq 1 "$REPS"); do
  raw="$OUT_DIR/native.rep$i.raw"
  err="$OUT_DIR/native.rep$i.err"
  if ! "${CAFF[@]}" "$DREAM" --release run "$BENCH" >"$raw" 2>"$err"; then
    echo "dream run failed on rep $i" >&2
    cat "$err" >&2
    tail -40 "$raw" >&2
    exit 1
  fi
  if grep -q '^panic:' "$raw" "$err"; then
    echo "dream panicked on rep $i" >&2
    grep '^panic:' "$raw" "$err" >&2
    exit 1
  fi
  grep '^bench ' "$raw" > "$OUT_DIR/native.rep$i.txt" || true
  if [[ ! -s "$OUT_DIR/native.rep$i.txt" ]]; then
    echo "rep $i produced no bench lines" >&2
    cat "$err" >&2
    tail -40 "$raw" >&2
    exit 1
  fi
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
  echo "== C# (dotnet -c Release) x$REPS =="
  if ! (cd "$CSHARP_DIR" && dotnet build -c Release --nologo -v q); then
    echo "C# build failed" >&2
    exit 1
  fi
  : > "$OUT_DIR/csharp.txt"
  for i in $(seq 1 "$REPS"); do
    raw="$OUT_DIR/csharp.rep$i.raw"
    if ! (cd "$CSHARP_DIR" \
      && DREAM_SCORES="$OUT_DIR/native.txt" "${CAFF[@]}" dotnet run -c Release --no-build --no-restore \
        >"$raw" 2>"$OUT_DIR/csharp.compare.txt"); then
      echo "C# run failed on rep $i" >&2
      cat "$OUT_DIR/csharp.compare.txt" >&2
      tail -40 "$raw" >&2
      exit 1
    fi
    grep '^bench ' "$raw" > "$OUT_DIR/csharp.rep$i.txt" || true
    if [[ ! -s "$OUT_DIR/csharp.rep$i.txt" ]]; then
      echo "C# rep $i produced no bench lines" >&2
      cat "$OUT_DIR/csharp.compare.txt" >&2
      tail -40 "$raw" >&2
      exit 1
    fi
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

# Suite order, with a section label on the first bench of each group.
SECTIONS = [
    ("compute", ["nbody", "mandelbrot", "matmul_64", "quicksort", "sieve"]),
    ("calls", ["fib_rec", "iface_dispatch"]),
    ("memory", ["binary_trees", "linked_walk"]),
    ("collections", ["wordcount", "parse_ints", "sum_options"]),
    ("strings", [
        "arc_locals", "string_concat", "string_eq", "char_scan", "byte_scan",
        "substring", "string_builder",
    ]),
    ("containers", [
        "list_push", "list_insert_mid", "map_get_set", "map_clear_reuse",
        "list_clear_reuse", "alloc_churn", "scratch_arena",
    ]),
    ("regex / json / simd", ["regex_find", "json_serialize", "json_deserialize", "arr_add", "vec_add"]),
]

def ns_per_op(line):
    kv = dict(tok.split("=", 1) for tok in line.split()[1:] if "=" in tok)
    if "iters" in kv:
        return float(kv["ns_total"]) / float(kv["iters"])
    return float(kv["ns_per_op"])

def load(path):
    out = {}
    p = Path(path)
    if not p.exists():
        return out
    for line in p.read_text().splitlines():
        if not line.startswith("bench "):
            continue
        try:
            out[line.split()[1]] = ns_per_op(line)
        except (KeyError, ValueError, ZeroDivisionError):
            continue
    return out

def spreads(stem, reps):
    """name -> spread percent across reps."""
    samples = {}
    out_dir = Path(sys.argv[1]).parent
    for i in range(1, reps + 1):
        p = out_dir / f"{stem}.rep{i}.txt"
        if not p.exists():
            continue
        for line in p.read_text().splitlines():
            if not line.startswith("bench "):
                continue
            try:
                samples.setdefault(line.split()[1], []).append(ns_per_op(line))
            except (KeyError, ValueError, ZeroDivisionError):
                continue
    out = {}
    for name, vs in samples.items():
        med = statistics.median(vs)
        out[name] = (max(vs) - min(vs)) / med * 100 if med else 0.0
    return out

def ns(v):
    return f"{'—':>12}" if v is None else f"{v:12,.1f}"

def spread_cell(sp):
    if sp is None:
        return f"{'—':>8}"
    mark = " !" if sp > 15 else "  "
    return f"{sp:5.0f}%{mark}"

def result(dream, csharp):
    if dream is None or csharp is None or dream <= 0 or csharp <= 0:
        return ""
    if csharp >= dream:
        ratio = csharp / dream
        if ratio < 1.05:
            return "about even"
        return f"Dream {ratio:.1f}× faster"
    ratio = dream / csharp
    if ratio < 1.05:
        return "about even"
    return f"C# {ratio:.1f}× faster"

reps = int(sys.argv[3])
dream = load(sys.argv[1])
csharp = load(sys.argv[2]) if len(sys.argv) > 2 else {}
dream_spread = spreads("native", reps)
csharp_spread = spreads("csharp", reps)

seen = set()
for _title, names in SECTIONS:
    for name in names:
        if name in dream or name in csharp:
            seen.add(name)
extras = sorted((set(dream) | set(csharp)) - seen)

print(f"Dream vs C# — median ns/op over {reps} run{'s' if reps != 1 else ''}")
print("± is (max − min) / median across those runs.  ! means the spread is above 15%.")
if not csharp:
    print("C# column is empty (dotnet bench did not run).")
print()
print(f"{'bench':<18} {'Dream ns/op':>12} {'C# ns/op':>12} {'Dream ±':>8} {'C# ±':>8}  result")
print(f"{'':-<18} {'':-<12} {'':-<12} {'':-<8} {'':-<8}  {'':-<22}")
shown = set()
for title, names in SECTIONS:
    rows = [n for n in names if n in dream or n in csharp]
    if not rows:
        continue
    print(f"  {title}")
    for name in rows:
        d = dream.get(name)
        c = csharp.get(name)
        print(
            f"{name:<18} {ns(d)} {ns(c)} {spread_cell(dream_spread.get(name))} "
            f"{spread_cell(csharp_spread.get(name))}  {result(d, c)}"
        )
        shown.add(name)
if extras:
    print("  other")
    for name in extras:
        d = dream.get(name)
        c = csharp.get(name)
        print(
            f"{name:<18} {ns(d)} {ns(c)} {spread_cell(dream_spread.get(name))} "
            f"{spread_cell(csharp_spread.get(name))}  {result(d, c)}"
        )
PY

echo "Wrote results under $OUT_DIR"

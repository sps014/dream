#!/usr/bin/env bash
# Source-generator compile-time benchmark: cold / warm-unchanged / warm-after-one-edit for a
# program without `@json`, a small `@json` program, and a 200-type `@json` program.
#
#   scripts/bench-generators.sh                 # print a table
#   scripts/bench-generators.sh --save FILE     # also write `scenario phase seconds` lines
#   scripts/bench-generators.sh --check FILE    # fail when any number exceeds FILE by >15%
#
# Uses `target/release/dream --emit-llvm`; every run gets a fresh `DREAM_BENCH_NONCE` so the
# whole-build cache never short-circuits the front end (generator caches are what is measured).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DREAM="${DREAM_BIN:-$ROOT/target/release/dream}"
GEN_CACHE="${DREAM_PREFIX:-$HOME/.dream}/cache/generators"
SAVE=""
CHECK=""
while [ $# -gt 0 ]; do
  case "$1" in
    --save) SAVE="$2"; shift 2 ;;
    --check) CHECK="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -x "$DREAM" ] || cargo build --release -q -p dream --manifest-path "$ROOT/Cargo.toml"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/dream-genbench.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

write_no_json() {
  mkdir -p "$WORK/no_json"
  cat >"$WORK/no_json/main.dream" <<'EOF'
import system;

class Point {
    public x: int;
    public y: int;
    public constructor(x: int, y: int) {
        this.x = x;
        this.y = y;
    }
}

fun main() {
    let p = Point(1, 2);
    System.println(p.x + p.y);
}
EOF
}

write_small_json() {
  mkdir -p "$WORK/small_json"
  cp "$ROOT/tests/cases/json_derive.dream" "$WORK/small_json/main.dream"
}

write_big_json() {
  mkdir -p "$WORK/big_json"
  {
    echo "import system;"
    echo "import system.json;"
    echo "import system.collections;"
    for i in $(seq 0 199); do
      echo "@json"
      echo "class Model$i {"
      echo "    public id: int;"
      echo "    public name: string;"
      echo "    public tags: List<string>;"
      echo "    public score: Option<double>;"
      echo "    public constructor(id: int, name: string, tags: List<string>, score: Option<double>) {"
      echo "        this.id = id;"
      echo "        this.name = name;"
      echo "        this.tags = tags;"
      echo "        this.score = score;"
      echo "    }"
      echo "}"
    done
    echo "fun main() {"
    echo "    let items = List<Model0>();"
    echo "    System.println(Json.serialize(items));"
    echo "}"
  } >"$WORK/big_json/main.dream"
}

now() { python3 -c 'import time; print(f"{time.perf_counter():.6f}")'; }

compile_once() {
  local file="$1"
  local t0 t1
  t0=$(now)
  DREAM_BENCH_NONCE="$RANDOM$RANDOM" "$DREAM" --emit-llvm -o "$WORK/out/$(basename "$(dirname "$file")").ll" "$file" >/dev/null 2>"$WORK/err.txt" || {
    cat "$WORK/err.txt" >&2
    echo "compile failed: $file" >&2
    exit 1
  }
  t1=$(now)
  python3 -c "print(f'{$t1 - $t0:.3f}')"
}

mkdir -p "$WORK/out"
write_no_json
write_small_json
write_big_json

RESULTS="$WORK/results.txt"
: >"$RESULTS"
printf '%-12s %10s %10s %10s\n' scenario cold warm one_edit
for scenario in no_json small_json big_json; do
  file="$WORK/$scenario/main.dream"
  rm -rf "$GEN_CACHE"
  cold=$(compile_once "$file")
  warm=$(compile_once "$file")
  printf '\nfun bench_edit_marker(): int { return %s; }\n' "$RANDOM" >>"$file"
  edit=$(compile_once "$file")
  printf '%-12s %10s %10s %10s\n' "$scenario" "$cold" "$warm" "$edit"
  {
    echo "$scenario cold $cold"
    echo "$scenario warm $warm"
    echo "$scenario one_edit $edit"
  } >>"$RESULTS"
done

if [ -n "$SAVE" ]; then
  cp "$RESULTS" "$SAVE"
  echo "saved $SAVE"
fi
if [ -n "$CHECK" ]; then
  python3 - "$CHECK" "$RESULTS" <<'EOF'
import sys
def load(p):
    out = {}
    for line in open(p):
        s, phase, v = line.split()
        out[(s, phase)] = float(v)
    return out
base, cur = load(sys.argv[1]), load(sys.argv[2])
bad = []
for key, value in sorted(cur.items()):
    ref = base.get(key)
    if ref is None:
        continue
    # Absolute slack absorbs process-spawn jitter on sub-second numbers.
    if value > ref * 1.15 + 0.05:
        bad.append(f"{key[0]} {key[1]}: {value:.3f}s > baseline {ref:.3f}s")
if bad:
    print("generator benchmark regressed:\n  " + "\n  ".join(bad))
    sys.exit(1)
print("generator benchmark: every number within baseline")
EOF
fi

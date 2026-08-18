#!/bin/bash
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DREAM="$ROOT/target/debug/dream"
cd "$ROOT"
ok=0
fail=0
skip=0
while IFS= read -r -d '' f; do
  stem="$(basename "$f" .dream)"
  if [[ -f "${f%.dream}.expected_error" ]]; then
    if "$DREAM" --native-c "$f" >/dev/null 2>&1; then
      echo "FAIL compile-should-err $stem"
      fail=$((fail + 1))
    else
      ok=$((ok + 1))
    fi
    continue
  fi
  if [[ ! -f "${f%.dream}.expected" && ! -f "${f%.dream}.expected_trap" ]]; then
    skip=$((skip + 1))
    continue
  fi
  out="$("$DREAM" --native-c run "$f" 2>&1)"
  st=$?
  if [[ $st -ne 0 ]]; then
    echo "FAIL run($st) $stem"
    echo "$out" | tail -3
    fail=$((fail + 1))
  else
    ok=$((ok + 1))
  fi
done < <(find tests/cases -maxdepth 1 -name '*.dream' -print0 | sort -z)
echo "ok=$ok fail=$fail skip=$skip"

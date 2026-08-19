#!/usr/bin/env bash
# Golden-corpus probe: `dream run` (native C) on tests/cases/*.dream.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
if [[ ! -x "$ROOT/target/debug/dream" ]]; then
  cargo build --manifest-path "$ROOT/Cargo.toml" -q -p dream
fi
exec python3 "$ROOT/scripts/probe_test.py" "$@"

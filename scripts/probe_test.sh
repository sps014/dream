#!/usr/bin/env bash
# Golden-corpus probe: native `dream run`, or `--node` for wasm32 + Node.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# Always rebuild: a warm incremental build is a few seconds, and probing a stale binary
# silently reports results for code that is no longer in the tree.
cargo build --manifest-path "$ROOT/Cargo.toml" -q -p dream --bin dream
exec python3 "$ROOT/scripts/probe_test.py" "$@"

#!/usr/bin/env bash
# Golden-corpus probe: native `dream run`, or `--node` for wasm32 + Node.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# Always rebuild: a warm incremental build is a few seconds, and probing a stale binary
# silently reports results for code that is no longer in the tree.
cargo build --manifest-path "$ROOT/Cargo.toml" --timings -q -p dream -p dream-host -p dream-host-core -p dream-host-unicode -p dream-host-crypto -p dream-host-process -p dream-host-timezone
exec "${PYTHON:-python3}" "$ROOT/scripts/probe_test.py" "$@"

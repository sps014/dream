#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
abi_temp="$(mktemp -d)"
trap 'rm -f "$abi_temp/generate-abi"; rmdir "$abi_temp"' EXIT
rustc --edition 2018 "$repo_root/crates/dream-mir/generate_abi.rs" -o "$abi_temp/generate-abi"
"$abi_temp/generate-abi" "$repo_root" "$@"

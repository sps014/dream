#!/usr/bin/env bash
# Put sccache on PATH for the rest of the job. The mozilla-actions/sccache-action download
# answers 504 often enough to fail the job, and its post step then looks for an executable
# named "undefined". A failed download turns the wrapper off so compilation still runs.
set -u

version="v0.16.0"
dest="${RUNNER_TEMP:-/tmp}/sccache-${version}"

disable_wrapper() {
  echo "sccache is unavailable; compiling without it" >&2
  if [[ -n "${GITHUB_ENV:-}" ]]; then
    echo "RUSTC_WRAPPER=" >> "$GITHUB_ENV"
    echo "SCCACHE_GHA_ENABLED=false" >> "$GITHUB_ENV"
  fi
  exit 0
}

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) target="x86_64-unknown-linux-musl" ;;
  Linux-aarch64 | Linux-arm64) target="aarch64-unknown-linux-musl" ;;
  Darwin-x86_64) target="x86_64-apple-darwin" ;;
  Darwin-arm64) target="aarch64-apple-darwin" ;;
  MINGW*-x86_64 | MSYS*-x86_64 | CYGWIN*-x86_64) target="x86_64-pc-windows-msvc" ;;
  MINGW*-aarch64 | MSYS*-aarch64 | CYGWIN*-aarch64) target="aarch64-pc-windows-msvc" ;;
  *) disable_wrapper ;;
esac

mkdir -p "$dest"
archive="$dest/sccache.tar.gz"
url="https://github.com/mozilla/sccache/releases/download/${version}/sccache-${version}-${target}.tar.gz"
if ! curl -fsSL --retry 5 --retry-all-errors --retry-delay 2 -o "$archive" "$url"; then
  disable_wrapper
fi
if ! tar -xzf "$archive" -C "$dest"; then
  disable_wrapper
fi

bin="$(find "$dest" -type f -name 'sccache' -o -type f -name 'sccache.exe' | head -n 1)"
if [[ -z "$bin" ]]; then
  disable_wrapper
fi
chmod +x "$bin"
bindir="$(dirname "$bin")"
if [[ -n "${GITHUB_PATH:-}" ]]; then
  echo "$bindir" >> "$GITHUB_PATH"
fi
export PATH="$bindir:$PATH"
# The release binary caches on local disk. The Actions cache token is only issued to the
# sccache setup action, so leaving SCCACHE_GHA_ENABLED set makes rustc fail at the first crate.
if [[ -n "${GITHUB_ENV:-}" ]]; then
  echo "SCCACHE_GHA_ENABLED=false" >> "$GITHUB_ENV"
  echo "SCCACHE_DIR=${RUNNER_TEMP:-/tmp}/sccache-cache" >> "$GITHUB_ENV"
fi
sccache --version

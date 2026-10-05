#!/usr/bin/env bash
# The full LLVM a development build needs (releases ship a minimal one instead): the official
# LLVM release — clang builds the C runtime, which a release ships prebuilt — plus two pieces of
# the wasi-sdk archive the LLVM release lacks: compiler-rt's wasm32 builtins, placed in clang's
# resource directory where `clang --target=wasm32-wasip1 -print-libgcc-file-name` finds them, and
# the WASI libc/C++ headers and archives (share/wasi-sysroot) for guest runtime and package code.
#
# Installs to ${DREAM_TOOLCHAINS:-~/.dream/toolchains}/llvm-<version>; the compiler finds it there.
# Idempotent. Usage: scripts/fetch-dev-llvm.sh
set -euo pipefail

LLVM_VERSION="22.1.8"
LLVM_MAJOR="${LLVM_VERSION%%.*}"
WASI_SDK_VERSION="33.0"
WASI_SDK_ARCHIVE="wasi-sdk-${WASI_SDK_VERSION}-x86_64-linux.tar.gz"
WASI_SDK_SHA="0ba8b5bfaeb2adf3f29bab5841d76cf5318ab8e1642ea195f88baba1abd47bce"

TOOLCHAINS="${DREAM_TOOLCHAINS:-${HOME}/.dream/toolchains}"
DEST="${TOOLCHAINS}/llvm-${LLVM_VERSION}"
TOOLS=(clang opt llc llvm-link llvm-dis llvm-as llvm-ar llvm-profdata lld wasm-ld)
EXE=""

[[ "$(uname -s)" == Linux || "$(uname -s)" == Darwin ]] && TOOLS+=("clang-${LLVM_MAJOR}")

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) LLVM_ARCHIVE="LLVM-${LLVM_VERSION}-Linux-X64.tar.xz"
    LLVM_SHA="df0e1ecf16caf3489a272a5eea4eec9b0d82878f6477fa309504f918a0006384" ;;
  Linux-aarch64|Linux-arm64) LLVM_ARCHIVE="LLVM-${LLVM_VERSION}-Linux-ARM64.tar.xz"
    LLVM_SHA="805efad2bb91cb4967fa569e0881d10c0f69c04461cf671cccbae19f547acc34" ;;
  Darwin-arm64) LLVM_ARCHIVE="LLVM-${LLVM_VERSION}-macOS-ARM64.tar.xz"
    LLVM_SHA="f260f4f7c0d430828a81ae8a3826a1d63fc0963ec2459489308cc23b1f7eab4f" ;;
  MINGW*-x86_64|MSYS*-x86_64|CYGWIN*-x86_64)
    LLVM_ARCHIVE="clang+llvm-${LLVM_VERSION}-x86_64-pc-windows-msvc.tar.xz"
    LLVM_SHA="d96c2cc1736f4eb7fa43cb9bbdf56d93551a9ae0a9aadb9c99c3c3b2b712a234"
    EXE=".exe"
    TOOLS+=(clang++ llvm-rc lld-link) ;;
  *) echo "no official LLVM ${LLVM_VERSION} build for this host; install it yourself and set DREAM_LLVM" >&2
     exit 1 ;;
esac

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
  else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

fetch() { # url file sha
  local out="${TOOLCHAINS}/$2"
  if [[ ! -f "$out" || "$(sha256 "$out")" != "$3" ]]; then
    echo "downloading $1" >&2
    curl -fL --retry 3 -o "$out.part" "$1"
    mv "$out.part" "$out"
  fi
  if [[ "$(sha256 "$out")" != "$3" ]]; then
    echo "checksum mismatch for $out" >&2
    exit 1
  fi
  echo "$out"
}

WILDCARDS=()
tar --version 2>/dev/null | grep -q GNU && WILDCARDS=(--wildcards)

mkdir -p "$TOOLCHAINS" "$DEST"

missing_tool=false
for t in "${TOOLS[@]}"; do
  [[ -x "$DEST/bin/$t$EXE" ]] || missing_tool=true
done
if $missing_tool; then
  archive="$(fetch "https://github.com/llvm/llvm-project/releases/download/llvmorg-${LLVM_VERSION}/${LLVM_ARCHIVE}" "$LLVM_ARCHIVE" "$LLVM_SHA")"
  patterns=("*/LICENSE.TXT" "*/lib/clang/*")
  for t in "${TOOLS[@]}"; do patterns+=("*/bin/$t$EXE"); done
  tar -xJf "$archive" -C "$DEST" --strip-components=1 ${WILDCARDS[@]+"${WILDCARDS[@]}"} "${patterns[@]}"
  rm -f "$archive"
fi

resource="$DEST/lib/clang/${LLVM_MAJOR}/lib"
if [[ ! -f "$resource/wasm32-unknown-wasip1/libclang_rt.builtins.a" \
   || ! -f "$resource/wasm32-unknown-wasip1-threads/libclang_rt.builtins.a" \
   || ! -f "$DEST/share/wasi-sysroot/include/wasm32-wasip1/eh/c++/v1/string" \
   || ! -f "$DEST/share/wasi-sysroot/lib/wasm32-wasip1/eh/libc++.a" \
   || ! -f "$DEST/share/wasi-sysroot/include/wasm32-wasip1-threads/eh/c++/v1/string" \
   || ! -f "$DEST/share/wasi-sysroot/lib/wasm32-wasip1-threads/eh/libc++.a" ]]; then
  archive="$(fetch "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-${WASI_SDK_VERSION%%.*}/${WASI_SDK_ARCHIVE}" "$WASI_SDK_ARCHIVE" "$WASI_SDK_SHA")"
  tmp="$(mktemp -d)"
  tar -xzf "$archive" -C "$tmp" --strip-components=1 ${WILDCARDS[@]+"${WILDCARDS[@]}"} \
    "*/lib/clang/${LLVM_MAJOR}/lib/wasm32-unknown-wasip1/libclang_rt.builtins.a" \
    "*/lib/clang/${LLVM_MAJOR}/lib/wasm32-unknown-wasip1-threads/libclang_rt.builtins.a" \
    "*/share/wasi-sysroot/include/wasm32-wasip1/*" \
    "*/share/wasi-sysroot/lib/wasm32-wasip1/*" \
    "*/share/wasi-sysroot/include/wasm32-wasip1-threads/*" \
    "*/share/wasi-sysroot/lib/wasm32-wasip1-threads/*"
  for t in wasm32-unknown-wasip1 wasm32-unknown-wasip1-threads; do
    mkdir -p "$resource/$t"
    cp "$tmp/lib/clang/${LLVM_MAJOR}/lib/$t/libclang_rt.builtins.a" "$resource/$t/"
  done
  rm -rf "$DEST/share/wasi-sysroot"
  mkdir -p "$DEST/share"
  mv "$tmp/share/wasi-sysroot" "$DEST/share/"
  rm -rf "$tmp" "$archive"
fi

"$DEST/bin/opt$EXE" --version | grep -q "LLVM version ${LLVM_VERSION}"
echo "LLVM ${LLVM_VERSION} ready at $DEST"

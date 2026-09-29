#!/usr/bin/env bash
# Build the minimal LLVM that ships inside the Dream release archive: opt, llc, llvm-link,
# llvm-dis, llvm-ar, llvm-profdata and lld (wasm-ld), plus llvm-rc on Windows, for the host
# target plus WebAssembly only.
#
# opt/llc/llvm-link/llvm-dis/llvm-profdata cannot join LLVM's multi-call `llvm` driver, so the
# tools share one `libLLVM` instead (each tool binary is then a few hundred KB). MSVC cannot build
# `libLLVM`, so Windows links the tools statically.
#
# Usage: scripts/build-llvm-dist.sh <llvm-project-src> <install-prefix>
#   LLVM_DIST_ARCH=x86_64|arm64   target CPU (default: this machine; macOS may cross via Rosetta)
#   BUILD_DIR=<dir>               CMake build tree (default: <install-prefix>.build)
set -euo pipefail

SRC="${1:?usage: build-llvm-dist.sh <llvm-project-src> <install-prefix>}"
PREFIX="${2:?usage: build-llvm-dist.sh <llvm-project-src> <install-prefix>}"
BUILD="${BUILD_DIR:-${PREFIX}.build}"
ARCH="${LLVM_DIST_ARCH:-$(uname -m)}"

case "$ARCH" in
  arm64 | aarch64) HOST_TARGET=AArch64 ;;
  x86_64 | AMD64 | amd64) HOST_TARGET=X86 ;;
  *) echo "unsupported arch: $ARCH" >&2; exit 1 ;;
esac

COMPONENTS="opt;llc;llvm-link;llvm-dis;llvm-ar;llvm-profdata;lld"
EXTRA=()
case "$(uname -s)" in
  MINGW* | MSYS* | CYGWIN*)
    # llvm-rc compiles the `.exe` icon resource for `--icon`.
    COMPONENTS="${COMPONENTS};llvm-rc"
    EXTRA+=(-DLLVM_BUILD_LLVM_DYLIB=OFF -DLLVM_LINK_LLVM_DYLIB=OFF
      -DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded)
    ;;
  Darwin)
    COMPONENTS="LLVM;${COMPONENTS}"
    EXTRA+=(-DLLVM_BUILD_LLVM_DYLIB=ON -DLLVM_LINK_LLVM_DYLIB=ON
      -DCMAKE_OSX_DEPLOYMENT_TARGET=11.0)
    if [[ "$ARCH" != "$(uname -m)" ]]; then
      EXTRA+=(-DCMAKE_OSX_ARCHITECTURES="$ARCH")
    fi
    ;;
  *)
    COMPONENTS="LLVM;${COMPONENTS}"
    EXTRA+=(-DLLVM_BUILD_LLVM_DYLIB=ON -DLLVM_LINK_LLVM_DYLIB=ON
      -DLLVM_STATIC_LINK_CXX_STDLIB=ON)
    ;;
esac

cmake -G Ninja -S "$SRC/llvm" -B "$BUILD" \
  -DCMAKE_BUILD_TYPE=MinSizeRel \
  -DCMAKE_INSTALL_PREFIX="$PREFIX" \
  -DLLVM_ENABLE_PROJECTS=lld \
  -DLLVM_TARGETS_TO_BUILD="${HOST_TARGET};WebAssembly" \
  -DLLVM_DISTRIBUTION_COMPONENTS="$COMPONENTS" \
  -DLLVM_ENABLE_ASSERTIONS=OFF \
  -DLLVM_ENABLE_BINDINGS=OFF \
  -DLLVM_ENABLE_ZLIB=OFF -DLLVM_ENABLE_ZSTD=OFF -DLLVM_ENABLE_LIBXML2=OFF \
  -DLLVM_ENABLE_LIBEDIT=OFF -DLLVM_ENABLE_LIBPFM=OFF \
  -DLLVM_ENABLE_CURL=OFF -DLLVM_ENABLE_HTTPLIB=OFF \
  -DLLVM_INCLUDE_TESTS=OFF -DLLVM_INCLUDE_EXAMPLES=OFF \
  -DLLVM_INCLUDE_BENCHMARKS=OFF -DLLVM_INCLUDE_DOCS=OFF \
  "${EXTRA[@]}"
ninja -C "$BUILD" install-distribution-stripped
cp "$SRC/llvm/LICENSE.TXT" "$PREFIX/LICENSE.TXT"

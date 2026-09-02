#!/bin/sh
# Install Dream toolchain (dream, dreamer, dream-lsp) from GitHub Releases.
#
#   curl --proto '=https' --tlsv1.2 -sSf https://sps014.github.io/dream/install.sh | sh
#
# POSIX /bin/sh so Debian/Ubuntu `dash` (the default `sh`) works when piped.
#
# Env:
#   DREAM_VERSION   optional tag without leading v (default: latest release)
#   DREAM_HOME      install prefix (default: ~/.dream). Re-runs replace bin/, lib/,
#                   and leftover files; toolchains/ (Zig) is kept.
#   DREAM_SKIP_CC=1 skip auto `dreamer toolchain install cc` when no compiler is found
#   DREAM_SKIP_LIBS=1 skip auto-install of Linux WebKitGTK / GTK runtime libraries

set -eu

REPO="${DREAM_REPO:-sps014/dream}"
# env.sh exports DREAM_HOME as ~/.dream/bin; treat that as the bin dir, not the prefix.
PREFIX="${DREAM_HOME:-${HOME}/.dream}"
case "$PREFIX" in
  */) PREFIX="${PREFIX%/}" ;;
esac
if [ "$(basename "$PREFIX")" = "bin" ]; then
  PREFIX="$(dirname "$PREFIX")"
fi
BIN_DIR="${PREFIX}/bin"
TMPDIR="${TMPDIR:-/tmp}"
WORK="$(mktemp -d "${TMPDIR%/}/dream-install.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

need() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "error: need '$1' on PATH" >&2
    exit 1
  }
}

need curl
need tar
need uname

detect_target() {
  os=
  arch=
  case "$(uname -s)" in
    Linux*) os=linux ;;
    Darwin*) os=macos ;;
    MINGW*|MSYS*|CYGWIN*) os=windows ;;
    *)
      echo "error: unsupported OS: $(uname -s)" >&2
      exit 1
      ;;
  esac
  case "$(uname -m)" in
    x86_64|amd64) arch=x64 ;;
    arm64|aarch64) arch=arm64 ;;
    *)
      echo "error: unsupported arch: $(uname -m)" >&2
      exit 1
      ;;
  esac
  echo "${os}-${arch}"
}

latest_tag() {
  json="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest")" || return 1
  echo "$json" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n1
}

TARGET="$(detect_target)"
if [ -n "${DREAM_VERSION:-}" ]; then
  TAG="v${DREAM_VERSION#v}"
else
  TAG="$(latest_tag || true)"
fi

if [ -z "${TAG}" ]; then
  cat >&2 <<EOF
error: no Dream release found on https://github.com/${REPO}/releases

Publish a tagged release (v0.1.0+) with toolchain archives, or build from source:

  git clone https://github.com/${REPO}.git
  cd dream
  source ./use-toolchain.sh
EOF
  exit 1
fi

VERSION="${TAG#v}"
if [ "$TARGET" = "windows-x64" ] || [ "$TARGET" = "windows-arm64" ]; then
  ARCHIVE="dream-${VERSION}-${TARGET}.zip"
  EXTRACT=zip
else
  ARCHIVE="dream-${VERSION}-${TARGET}.tar.gz"
  EXTRACT=tar
fi

BASE="https://github.com/${REPO}/releases/download/${TAG}"
URL="${BASE}/${ARCHIVE}"
SUM_URL="${BASE}/SHA256SUMS"

echo "Installing Dream ${VERSION} (${TARGET}) → ${PREFIX}"
echo "Downloading ${URL}"

if ! curl -fsSL "$URL" -o "${WORK}/${ARCHIVE}"; then
  cat >&2 <<EOF
error: failed to download ${URL}

Is ${TAG} published with asset ${ARCHIVE}?
See https://github.com/${REPO}/releases
EOF
  exit 1
fi

if curl -fsSL "$SUM_URL" -o "${WORK}/SHA256SUMS" 2>/dev/null; then
  EXPECTED="$(grep -E "  ${ARCHIVE}\$" "${WORK}/SHA256SUMS" | awk '{print $1}' || true)"
  if [ -n "$EXPECTED" ]; then
    if command -v shasum >/dev/null 2>&1; then
      ACTUAL="$(shasum -a 256 "${WORK}/${ARCHIVE}" | awk '{print $1}')"
    elif command -v sha256sum >/dev/null 2>&1; then
      ACTUAL="$(sha256sum "${WORK}/${ARCHIVE}" | awk '{print $1}')"
    else
      echo "warning: no shasum/sha256sum; skipping checksum verify" >&2
      ACTUAL=""
    fi
    if [ -n "$ACTUAL" ] && [ "$ACTUAL" != "$EXPECTED" ]; then
      echo "error: checksum mismatch for ${ARCHIVE}" >&2
      echo "  expected ${EXPECTED}" >&2
      echo "  got      ${ACTUAL}" >&2
      exit 1
    fi
    if [ -n "$ACTUAL" ]; then
      echo "Checksum OK"
    fi
  fi
fi

mkdir -p "${WORK}/out"
if [ "$EXTRACT" = tar ]; then
  tar -xzf "${WORK}/${ARCHIVE}" -C "${WORK}/out"
else
  need unzip
  unzip -q "${WORK}/${ARCHIVE}" -d "${WORK}/out"
fi

# Drop the previous compiler/runtime payload so leftover binaries and headers
# cannot mix with this version. Keep `toolchains/` (Zig / wasi-sdk).
replace_previous_install() {
  [ -d "$PREFIX" ] || return 0
  echo "Removing previous Dream install under ${PREFIX} (keeping toolchains/)"
  for entry in "$PREFIX"/*; do
    [ -e "$entry" ] || continue
    name="${entry##*/}"
    case "$name" in
      toolchains|toolchains.env) continue ;;
    esac
    rm -rf "$entry"
  done
}

_extracted_dream="$(find "${WORK}/out" -type f \( -name dream -o -name 'dream.exe' \) | head -n1 || true)"
if [ -z "$_extracted_dream" ]; then
  echo "error: archive did not contain a dream binary" >&2
  exit 1
fi

replace_previous_install
mkdir -p "$BIN_DIR"

# Archives contain binaries + libdream at top level or in a single directory.
copy_named() {
  find "${WORK}/out" -type f \( \
    -name 'dream' -o -name 'dream.exe' -o \
    -name 'dreamer' -o -name 'dreamer.exe' -o \
    -name 'dream-lsp' -o -name 'dream-lsp.exe' -o \
    -name 'libdream.so' -o -name 'libdream.dylib' -o \
    -name 'dream.dll' -o -name 'dream.dll.lib' -o -name 'dream.lib' -o \
    -name 'libdream.dll.a' \
  \) -exec cp -f {} "$BIN_DIR/" \;
}

copy_named

chmod +x "${BIN_DIR}/dream" "${BIN_DIR}/dreamer" "${BIN_DIR}/dream-lsp" 2>/dev/null || true

EXT=
case "$TARGET" in
  windows-*) EXT=.exe ;;
esac

if [ ! -f "${BIN_DIR}/dream${EXT}" ] || [ ! -f "${BIN_DIR}/dreamer${EXT}" ]; then
  echo "error: archive did not contain dream/dreamer binaries" >&2
  exit 1
fi

LIBDREAM_OK=0
for lib in libdream.so libdream.dylib dream.dll; do
  if [ -f "${BIN_DIR}/${lib}" ]; then
    LIBDREAM_OK=1
    break
  fi
done
if [ "$LIBDREAM_OK" -eq 0 ]; then
  echo "error: archive did not contain libdream (needed to link native programs)" >&2
  echo "  expected libdream.so, libdream.dylib, or dream.dll next to the compiler" >&2
  exit 1
fi

# Native-C runtime sources (packed next to the binaries in the release archive).
RT_SRC="$(find "${WORK}/out" -type d -path '*/lib/runtime/c' 2>/dev/null | head -n1 || true)"
if [ -n "$RT_SRC" ] && [ -f "${RT_SRC}/native/include/dream_rt_native.h" ]; then
  mkdir -p "${PREFIX}/lib/runtime"
  rm -rf "${PREFIX}/lib/runtime/c"
  cp -R "$RT_SRC" "${PREFIX}/lib/runtime/c"
fi

env_compiler() {
  _v="$1"
  [ -n "$_v" ] || return 1
  if [ -f "$_v" ]; then
    return 0
  fi
  command -v "$_v" >/dev/null 2>&1
}

has_toolchain_zig() {
  for cand in "${PREFIX}/toolchains"/zig-*/zig "${PREFIX}/toolchains"/zig-*/zig.exe; do
    if [ -f "$cand" ]; then
      return 0
    fi
  done
  return 1
}

has_cc() {
  env_compiler "${DREAM_CC:-}" && return 0
  env_compiler "${CC:-}" && return 0
  env_compiler "${DREAM_ZIG:-}" && return 0
  has_toolchain_zig && return 0
  command -v cc >/dev/null 2>&1 && return 0
  command -v clang >/dev/null 2>&1 && return 0
  command -v zig >/dev/null 2>&1 && return 0
  return 1
}

LIBS_NOTE=
bin_needs_shared_libs() {
  _bin="$1"
  [ -f "$_bin" ] || return 1
  command -v ldd >/dev/null 2>&1 || return 1
  ldd "$_bin" 2>/dev/null | grep -q 'not found'
}

linux_pkg_install() {
  _pkgs="$1"
  _sudo=
  if [ "$(id -u)" -ne 0 ]; then
    if command -v sudo >/dev/null 2>&1; then
      _sudo=sudo
    else
      return 1
    fi
  fi
  if command -v apt-get >/dev/null 2>&1; then
    export DEBIAN_FRONTEND=noninteractive
    # shellcheck disable=SC2086
    ${_sudo} apt-get update -qq && ${_sudo} apt-get install -y -qq --no-install-recommends ${_pkgs}
    return $?
  fi
  if command -v dnf >/dev/null 2>&1; then
    # shellcheck disable=SC2086
    ${_sudo} dnf install -y ${_pkgs}
    return $?
  fi
  if command -v pacman >/dev/null 2>&1; then
    # shellcheck disable=SC2086
    ${_sudo} pacman -S --noconfirm --needed ${_pkgs}
    return $?
  fi
  return 1
}

linux_runtime_packages() {
  if command -v apt-get >/dev/null 2>&1; then
    echo "libwebkit2gtk-4.1-0 libgtk-3-0 libudev1"
  elif command -v dnf >/dev/null 2>&1; then
    echo "webkit2gtk4.1 gtk3 systemd-libs"
  elif command -v pacman >/dev/null 2>&1; then
    echo "webkit2gtk-4.1 gtk3"
  else
    echo ""
  fi
}

ensure_linux_libs() {
  case "$TARGET" in
    linux-*) ;;
    *) return 0 ;;
  esac
  if [ "${DREAM_SKIP_LIBS:-}" = "1" ]; then
    LIBS_NOTE="Skipped Linux runtime libraries (DREAM_SKIP_LIBS=1)"
    echo "${LIBS_NOTE}"
    return 0
  fi
  if ! bin_needs_shared_libs "${BIN_DIR}/dream${EXT}" && ! bin_needs_shared_libs "${BIN_DIR}/libdream.so"; then
    return 0
  fi
  _pkgs="$(linux_runtime_packages)"
  if [ -z "$_pkgs" ]; then
    LIBS_NOTE="warning: dream needs WebKitGTK (libwebkit2gtk-4.1) and GTK 3; install them with your package manager"
    echo "${LIBS_NOTE}" >&2
    return 0
  fi
  echo "Installing Linux runtime libraries for native run / system.webview: ${_pkgs}"
  if linux_pkg_install "${_pkgs}"; then
    if bin_needs_shared_libs "${BIN_DIR}/dream${EXT}" || bin_needs_shared_libs "${BIN_DIR}/libdream.so"; then
      LIBS_NOTE="warning: shared libraries still missing after install; see ldd ${BIN_DIR}/dream"
      echo "${LIBS_NOTE}" >&2
      ldd "${BIN_DIR}/dream${EXT}" 2>/dev/null | grep 'not found' >&2 || true
    else
      LIBS_NOTE="Installed Linux runtime libraries (${_pkgs})"
    fi
  else
    LIBS_NOTE="warning: could not install ${_pkgs}; install them so dream and system.webview can load"
    echo "${LIBS_NOTE}" >&2
  fi
}

CC_NOTE=
ensure_cc() {
  if [ "${DREAM_SKIP_CC:-}" = "1" ]; then
    CC_NOTE="Skipped C compiler install (DREAM_SKIP_CC=1)"
    echo "${CC_NOTE}"
    return 0
  fi
  if has_cc; then
    CC_NOTE="C compiler already found; skipped dreamer toolchain install cc"
    echo "${CC_NOTE}"
    return 0
  fi
  echo "No C compiler on PATH; installing via dreamer toolchain install cc"
  if "${BIN_DIR}/dreamer${EXT}" toolchain install cc; then
    CC_NOTE="Installed C compiler (Zig) via dreamer toolchain install cc"
  else
    CC_NOTE="warning: could not install a C compiler; later run: dreamer toolchain install cc"
    echo "${CC_NOTE}" >&2
  fi
}

cat > "${PREFIX}/toolchain.env" <<EOF
DREAM_HOME=${BIN_DIR}
DREAMER_HOME=${BIN_DIR}
DREAM_BIN=${BIN_DIR}/dream${EXT}
EOF

cat > "${PREFIX}/env.sh" <<EOF
# Dream toolchain
export DREAM_HOME="${BIN_DIR}"
export DREAMER_HOME="${BIN_DIR}"
export DREAM_BIN="${BIN_DIR}/dream${EXT}"
if [ -f "${PREFIX}/toolchains.env" ]; then
  set -a
  # shellcheck disable=SC1091
  . "${PREFIX}/toolchains.env"
  set +a
fi
case ":\$PATH:" in
  *":${BIN_DIR}:"*) ;;
  *) export PATH="${BIN_DIR}:\$PATH" ;;
esac
EOF

MARKER="# Dream toolchain (install.sh)"
add_rc_hook() {
  rc="$1"
  [ -f "$rc" ] || touch "$rc"
  if grep -Fq "$MARKER" "$rc" 2>/dev/null; then
    return 0
  fi
  printf '\n%s\n. "%s/env.sh"\n' "$MARKER" "$PREFIX" >>"$rc"
  echo "Added PATH hook to ${rc}"
}

case "${SHELL:-}" in
  */zsh) add_rc_hook "${HOME}/.zshrc" ;;
  */bash) add_rc_hook "${HOME}/.bashrc" ;;
  *)
    add_rc_hook "${HOME}/.zshrc"
    add_rc_hook "${HOME}/.bashrc"
    ;;
esac

export DREAM_HOME="${BIN_DIR}"
export DREAMER_HOME="${BIN_DIR}"
export DREAM_BIN="${BIN_DIR}/dream${EXT}"
case ":${PATH}:" in
  *":${BIN_DIR}:"*) ;;
  *) export PATH="${BIN_DIR}:${PATH}" ;;
esac

ensure_linux_libs
ensure_cc

echo
echo "Installed:"
echo "  ${BIN_DIR}/dream${EXT}"
echo "  ${BIN_DIR}/dreamer${EXT}"
echo "  ${BIN_DIR}/dream-lsp${EXT}"
if [ -n "${LIBS_NOTE}" ]; then
  echo "  ${LIBS_NOTE}"
fi
if [ -n "${CC_NOTE}" ]; then
  echo "  ${CC_NOTE}"
fi
echo
echo "Open a new terminal (or: . ${PREFIX}/env.sh), then:"
echo "  dream --help"
echo "  dreamer --help"
echo "  dreamer init hello && cd hello && dreamer run"

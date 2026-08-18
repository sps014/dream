#!/usr/bin/env bash
# Compile crates/dream-mir/src/runtime/c/*.c to wasm32 objects and extract named WAT functions.
#
#   scripts/build-runtime.sh           # write c/generated/*.wat (does not splice into dream)
#   scripts/build-runtime.sh --check   # skip if no wasm32 clang; else fail on extract-gate errors
#
# Does NOT run during `cargo build` / `dream`. Windows CI must not invoke this.
# Pin: wasi-sdk 33 clang (not Apple/Xcode clang). Optional wasm2wat / wasm-tools / wasm-opt.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
C_DIR="$ROOT/crates/dream-mir/src/runtime/c"
INC="$C_DIR/include"
OUT_DIR="$C_DIR/generated"
CHECK=0
if [[ "${1:-}" == "--check" ]]; then
  CHECK=1
fi

die() { echo "build-runtime: $*" >&2; exit 1; }

# Prefer an explicit SDK, then a local install from the README, then PATH clang.
find_clang() {
  if [[ -n "${WASI_SDK_PATH:-}" && -x "${WASI_SDK_PATH}/bin/clang" ]]; then
    echo "${WASI_SDK_PATH}/bin/clang"
    return
  fi
  local home_sdk
  home_sdk="$(echo "$HOME"/.dream/toolchains/wasi-sdk-*/bin/clang 2>/dev/null | tail -1)"
  if [[ -n "$home_sdk" && -x "$home_sdk" ]]; then
    echo "$home_sdk"
    return
  fi
  command -v clang
}

need_clang() {
  local clang
  clang="$(find_clang || true)"
  if [[ -z "$clang" ]]; then
    if [[ "$CHECK" -eq 1 ]]; then
      echo "build-runtime --check: clang not found; skipping (WAT is checked in)."
      exit 0
    fi
    die "clang not found. Install wasi-sdk 33; see crates/dream-mir/src/runtime/README.md"
  fi
  if ! echo 'int x;' | "$clang" --target=wasm32 -nostdlib -c -o /dev/null -x c - >/dev/null 2>&1; then
    if [[ "$CHECK" -eq 1 ]]; then
      echo "build-runtime --check: clang has no wasm32 target (Apple/Xcode clang is not enough; use wasi-sdk). Skipping."
      exit 0
    fi
    die "clang cannot target wasm32. Install wasi-sdk 33 (https://github.com/WebAssembly/wasi-sdk)."
  fi
  echo "$clang"
}

FORBIDDEN='__stack_pointer|__memory_base|__table_base|__wasm_call_ctors|\bmemcpy\b|\bmemmove\b'

print_wat() {
  local obj="$1"
  if command -v wasm-tools >/dev/null 2>&1; then
    wasm-tools print "$obj"
  elif command -v wasm2wat >/dev/null 2>&1; then
    wasm2wat --enable-threads "$obj"
  else
    die "need wasm-tools or wasm2wat to print relocatable wasm"
  fi
}

# Relocatable objects import env.NAME as $rt_NAME (C identifier). Rewrite calls to $NAME.
rewrite_imports() {
  python3 - "$1" <<'PY'
import re, sys
path = sys.argv[1]
text = open(path, encoding="utf-8").read()
for m in re.finditer(
    r'\(import\s+"env"\s+"([^"]+)"\s+\(func\s+(\$[^\s)]+)',
    text,
):
    wasm_name, local = m.group(1), m.group(2)
    want = "$" + wasm_name
    if local != want:
        text = text.replace("call " + local, "call " + want)
print(text)
PY
}

extract_funcs() {
  python3 -c '
import re, sys
text = sys.stdin.read()
funcs = []
i = 0
while True:
    m = re.search(r"\(func\b", text[i:])
    if not m:
        break
    start = i + m.start()
    depth = 0
    j = start
    while j < len(text):
        if text[j] == "(":
            depth += 1
        elif text[j] == ")":
            depth -= 1
            if depth == 0:
                j += 1
                break
        j += 1
    chunk = text[start:j].strip()
    line_start = text.rfind("\n", 0, start) + 1
    prefix = text[line_start:start]
    if chunk.startswith("(func") and prefix.strip() == "" and re.match(r"\(func\s+\$", chunk):
        funcs.append(chunk)
    i = j
print("\n\n".join(funcs))
'
}

CLANG="$(need_clang)"
echo "build-runtime: using $CLANG"
mkdir -p "$OUT_DIR"

# Relocatable objects (-c): no linker-injected $__stack_pointer / ctors.
CFLAGS=(
  --target=wasm32
  -nostdlib
  -O3
  -ffunction-sections
  -fno-builtin
  -fno-exceptions
  -mbulk-memory
  -matomics
  -I "$INC"
  -c
)

failed=0
for src in "$C_DIR"/*.c; do
  base="$(basename "$src" .c)"
  if [[ "$base" == regex || "$base" == regex_wasm_libc ]]; then
    continue
  fi
  obj="$OUT_DIR/${base}.o"
  wat="$OUT_DIR/${base}.wat"
  echo "build-runtime: $base"
  if ! "$CLANG" "${CFLAGS[@]}" -o "$obj" "$src" 2>"$OUT_DIR/${base}.clang.err"; then
    echo "build-runtime: clang failed for $base:" >&2
    cat "$OUT_DIR/${base}.clang.err" >&2
    failed=1
    continue
  fi
  if ! printed="$(print_wat "$obj" 2>"$OUT_DIR/${base}.print.err")"; then
    echo "build-runtime: wasm print failed for $base:" >&2
    cat "$OUT_DIR/${base}.print.err" >&2
    failed=1
    continue
  fi
  printf '%s\n' "$printed" > "$OUT_DIR/${base}.full.wat"
  rewritten="$(rewrite_imports "$OUT_DIR/${base}.full.wat")"
  printf '%s\n' "$rewritten" > "$OUT_DIR/${base}.full.wat"
  if echo "$rewritten" | grep -E "$FORBIDDEN" >/dev/null; then
    echo "build-runtime: extract gate failed for $base (stack pointer / memcpy / ctors)." >&2
    echo "  Handwritten crates/dream-mir/src/runtime/${base}.wat stays the emit artifact." >&2
    failed=1
    continue
  fi
  printf '%s\n' "$rewritten" | extract_funcs > "$wat"
  if [[ ! -s "$wat" ]]; then
    echo "build-runtime: no funcs extracted from $base" >&2
    failed=1
    continue
  fi
done

# PCRE2 interpreter: same source list as native (`pcre2/SOURCES`). JIT is native-only.
PCRE2_DIR="$C_DIR/pcre2"
WASM_LD="$(dirname "$CLANG")/wasm-ld"
PCRE2_CFLAGS=(
  --target=wasm32-wasi
  -nostdlib
  -O3
  -fno-builtin
  -fno-exceptions
  -mbulk-memory
  --sysroot="$(dirname "$(dirname "$CLANG")")/share/wasi-sysroot"
  -DHAVE_CONFIG_H
  -DPCRE2_CODE_UNIT_WIDTH=16
  -DPCRE2_STATIC
  -DPCRE2_WASM
  -I "$PCRE2_DIR"
  -I "$INC"
  -c
)
pcre2_objs=()
while IFS= read -r name || [[ -n "$name" ]]; do
  name="${name%%#*}"
  name="$(echo "$name" | tr -d '[:space:]')"
  [[ -z "$name" ]] && continue
  echo "build-runtime: pcre2 $name"
  obj="$OUT_DIR/pcre2_${name%.c}.o"
  if ! "$CLANG" "${PCRE2_CFLAGS[@]}" -o "$obj" "$PCRE2_DIR/$name" 2>"$OUT_DIR/pcre2_${name}.err"; then
    echo "build-runtime: clang failed for pcre2 $name:" >&2
    cat "$OUT_DIR/pcre2_${name}.err" >&2
    failed=1
    continue
  fi
  pcre2_objs+=("$obj")
done < "$PCRE2_DIR/SOURCES"
echo "build-runtime: regex.wasm libc + wrapper"
if ! "$CLANG" "${PCRE2_CFLAGS[@]}" -o "$OUT_DIR/regex_wasm_libc.o" "$C_DIR/regex_wasm_libc.c" 2>"$OUT_DIR/regex_wasm_libc.err"; then
  cat "$OUT_DIR/regex_wasm_libc.err" >&2
  failed=1
fi
if ! "$CLANG" "${PCRE2_CFLAGS[@]}" -o "$OUT_DIR/regex.o" "$C_DIR/regex.c" 2>"$OUT_DIR/regex.clang.err"; then
  cat "$OUT_DIR/regex.clang.err" >&2
  failed=1
fi
if [[ "$failed" -eq 0 && -x "$WASM_LD" ]]; then
  echo "build-runtime: link regex.wasm"
  # --global-base matches abi::LINKED_RT_BASE. C stack is relocated on WAT ingest.
  if ! "$WASM_LD" --no-entry --allow-undefined --import-memory \
    --wrap=malloc --wrap=free --wrap=realloc --wrap=memcpy --wrap=memmove \
    --wrap=memset --wrap=memcmp --wrap=strlen --wrap=abort \
    --export=regex_compile --export=regex_free --export=regex_group_count \
    --export=regex_name_count --export=regex_name_at --export=regex_name_number \
    --export=regex_find --export=regex_test \
    --global-base=2097152 -z stack-size=131072 \
    -o "$OUT_DIR/regex.wasm" \
    "${pcre2_objs[@]}" "$OUT_DIR/regex_wasm_libc.o" "$OUT_DIR/regex.o" \
    2>"$OUT_DIR/regex.link.err"; then
    echo "build-runtime: wasm-ld regex failed:" >&2
    cat "$OUT_DIR/regex.link.err" >&2
    failed=1
  else
    printed="$(print_wat "$OUT_DIR/regex.wasm")"
    printf '%s\n' "$printed" > "$OUT_DIR/regex.full.wat"
    rewritten="$(rewrite_imports "$OUT_DIR/regex.full.wat")"
    printf '%s\n' "$rewritten" > "$OUT_DIR/regex.full.wat"
    python3 - "$OUT_DIR/regex.full.wat" "$ROOT/crates/dream-mir/src/runtime/regex.wat" <<'PY'
import sys
src, dst = sys.argv[1], sys.argv[2]
text = open(src, encoding="utf-8").read()
banner = ";; Generated by scripts/build-runtime.sh from runtime/c + pcre2/SOURCES. Edit the .c, not this file.\n"
open(dst, "w", encoding="utf-8").write(banner + text)
print("build-runtime: promoted", dst)
PY
  fi
elif [[ "$failed" -eq 0 ]]; then
  echo "build-runtime: wasm-ld not found at $WASM_LD; skip PCRE2 wasm link" >&2
  failed=1
fi

if [[ "$failed" -ne 0 ]]; then
  if [[ "$CHECK" -eq 1 ]]; then
    die "clang/extract gates failed"
  fi
  die "some units failed"
fi

# Guest emit artifacts: promote C-extracted funcs. Allocator stays handwritten
# (`;;@DEBUG_*@` / `;;@ALLOC_LOCK_*@`). Regex is promoted on the PCRE2 link path.
promote_guest() {
  local src="$1"
  local dest="$2"
  python3 - "$src" "$dest" <<'PY'
import re, sys
src, dest = sys.argv[1], sys.argv[2]
text = open(src, encoding="utf-8").read()
text = re.sub(r"\s*\(type \d+\)", "", text)
text = text.replace("$strlen_dream", "$strlen")
banner = ";; Generated by scripts/build-runtime.sh from runtime/c. Edit the .c, not this file.\n"
open(dest, "w", encoding="utf-8").write(banner + text)
print("build-runtime: promoted", dest)
PY
}
RT="$ROOT/crates/dream-mir/src/runtime"
promote_guest "$OUT_DIR/strings.wat" "$RT/strings.wat"
promote_guest "$OUT_DIR/object.wat" "$RT/object.wat"
promote_guest "$OUT_DIR/format.wat" "$RT/format.wat"

echo "build-runtime: wrote $OUT_DIR/*.wat and promoted strings/object/format/regex"
echo "  allocator.wat stays handwritten (debug/thread placeholders)."
if [[ "$CHECK" -eq 1 ]]; then
  echo "build-runtime --check: ok"
fi

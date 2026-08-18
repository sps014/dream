#!/usr/bin/env bash
# Link catalogued C libraries (PCRE2 regex) to wasm32 and write runtime/*.wat.
#
#   scripts/build-runtime.sh         # write c/generated/regex.wasm + promote regex.wat
#   scripts/build-runtime.sh --check # skip if no wasm32 clang; else fail on link errors
#
# Does NOT run during `cargo build` / `dream`. Windows CI must not invoke this.
# Same-module guest helpers are authored as runtime/*.wat — this script does not touch them.
# Pin: wasi-sdk 33 clang (not Apple/Xcode clang). Optional wasm2wat / wasm-tools.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CHECK=0
if [[ "${1:-}" == "--check" ]]; then
  CHECK=1
fi

die() { echo "build-runtime: $*" >&2; exit 1; }

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

CLANG="$(need_clang)"
echo "build-runtime: using $CLANG"

C_DIR="$ROOT/crates/dream-mir/src/runtime/c"
RT_DIR="$ROOT/crates/dream-mir/src/runtime"
OUT_DIR="$C_DIR/generated"
mkdir -p "$OUT_DIR"

echo "build-runtime: writing catalog manifest"
if ! cargo run -q -p dream-mir --bin dream-runtime-manifest --manifest-path "$ROOT/Cargo.toml" \
  > "$OUT_DIR/runtime-manifest.json"; then
  die "dream-runtime-manifest failed"
fi

WASM_LD="$(dirname "$CLANG")/wasm-ld"
SYSROOT="$(dirname "$(dirname "$CLANG")")/share/wasi-sysroot"
if [[ ! -x "$WASM_LD" ]]; then
  die "wasm-ld not found at $WASM_LD"
fi

python3 - "$OUT_DIR" "$CLANG" "$WASM_LD" "$SYSROOT" "$C_DIR" "$RT_DIR" <<'PY'
import json, os, subprocess, sys

out_dir, clang, wasm_ld, sysroot, c_dir, rt_dir = sys.argv[1:7]
manifest = json.load(open(os.path.join(out_dir, "runtime-manifest.json"), encoding="utf-8"))
failed = 0

def run(cmd, err_path):
    p = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    if p.returncode != 0:
        open(err_path, "w", encoding="utf-8").write(p.stderr or p.stdout or "")
    return p

def print_wat(wasm):
    for tool in (("wasm-tools", "print", wasm), ("wasm2wat", "--enable-threads", wasm)):
        try:
            p = subprocess.run(list(tool), stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        except FileNotFoundError:
            continue
        if p.returncode == 0:
            return p.stdout
        raise SystemExit("wasm print failed: " + (p.stderr or ""))
    raise SystemExit("need wasm-tools or wasm2wat to print linked wasm")

for mod in manifest["modules"]:
    mid = mod["id"]
    print("build-runtime: link", mid)
    objs = []
    cflags = [
        "--target=wasm32-wasi", "-nostdlib", "-O3", "-fno-builtin", "-fno-exceptions",
        "-mbulk-memory", "--sysroot=" + sysroot, "-c",
    ]
    for d in mod["include_dirs"]:
        cflags.extend(["-I", os.path.join(c_dir, d)])
    for d in mod["wasm_defines"]:
        cflags.append("-D" + d)
    for rel in mod["wasm_c"]:
        src = os.path.join(c_dir, rel)
        base = rel.replace("/", "_").replace(".c", "")
        obj = os.path.join(out_dir, base + ".o")
        err = os.path.join(out_dir, base + ".err")
        p = run([clang] + cflags + ["-o", obj, src], err)
        if p.returncode != 0:
            sys.stderr.write(open(err, encoding="utf-8").read())
            failed = 1
            continue
        objs.append(obj)
    if failed:
        continue
    wasm = os.path.join(out_dir, mid + ".wasm")
    cmd = [wasm_ld, "--no-entry", "--allow-undefined", "--import-memory"]
    for w in mod["wrap"]:
        cmd.append("--wrap=" + w)
    for e in mod["exports"]:
        cmd.append("--export=" + e)
    cmd.extend([
        "--global-base=" + str(mod["global_base"]),
        "-z", "stack-size=" + str(mod["stack_size"]),
        "-o", wasm,
    ])
    cmd.extend(objs)
    err = os.path.join(out_dir, mid + ".link.err")
    p = run(cmd, err)
    if p.returncode != 0:
        print("build-runtime: wasm-ld %s failed:" % mid, file=sys.stderr)
        sys.stderr.write(open(err, encoding="utf-8").read())
        failed = 1
        continue
    try:
        printed = print_wat(wasm)
    except SystemExit as e:
        print(e, file=sys.stderr)
        failed = 1
        continue
    open(os.path.join(out_dir, mid + ".full.wat"), "w", encoding="utf-8").write(printed)
    dest = os.path.join(rt_dir, mod["wat_out"])
    banner = ";; Generated by scripts/build-runtime.sh from runtime/c (linked). Edit the .c, not this file.\n"
    body = banner + printed
    if not body.endswith("\n"):
        body += "\n"
    open(dest, "w", encoding="utf-8").write(body)
    print("build-runtime: wrote", dest)

sys.exit(failed)
PY
status=$?
if [[ "$status" -ne 0 ]]; then
  if [[ "$CHECK" -eq 1 ]]; then
    die "clang/link failed"
  fi
  die "link failed"
fi

echo "build-runtime: wrote linked WAT under $RT_DIR"
if [[ "$CHECK" -eq 1 ]]; then
  echo "build-runtime --check: ok"
fi

#!/usr/bin/env bash
# Compile catalogued C runtime modules to wasm32 and promote WAT.
#
#   scripts/build-runtime.sh           # write generated/*.wat and promote catalog wat_out
#   scripts/build-runtime.sh --check   # skip if no wasm32 clang; else fail on extract-gate errors
#
# Does NOT run during `cargo build` / `dream`. Windows CI must not invoke this.
# Pin: wasi-sdk 33 clang (not Apple/Xcode clang). Optional wasm2wat / wasm-tools / wasm-opt.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
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

python3 - "$OUT_DIR" "$CLANG" "$WASM_LD" "$SYSROOT" "$C_DIR" "$RT_DIR" "$FORBIDDEN" <<'PY'
import json, os, subprocess, sys

out_dir, clang, wasm_ld, sysroot, c_dir, rt_dir, forbidden = sys.argv[1:8]
manifest = json.load(open(os.path.join(out_dir, "runtime-manifest.json"), encoding="utf-8"))
failed = 0

def run(cmd, err_path):
    p = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    if p.returncode != 0:
        open(err_path, "w", encoding="utf-8").write(p.stderr or p.stdout or "")
    return p

def print_wat(obj):
    for tool in (("wasm-tools", "print", obj), ("wasm2wat", "--enable-threads", obj)):
        try:
            p = subprocess.run(list(tool), stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        except FileNotFoundError:
            continue
        if p.returncode == 0:
            return p.stdout
        raise SystemExit("wasm print failed: " + (p.stderr or ""))
    raise SystemExit("need wasm-tools or wasm2wat to print relocatable wasm")

def rewrite_imports(text):
    import re
    for m in re.finditer(r'\(import\s+"env"\s+"([^"]+)"\s+\(func\s+(\$[^\s)]+)', text):
        wasm_name, local = m.group(1), m.group(2)
        want = "$" + wasm_name
        if local != want:
            text = text.replace("call " + local, "call " + want)
    return text

def extract_funcs(text):
    import re
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
    return "\n\n".join(funcs)

def inc_flags(mod):
    flags = []
    for d in mod["include_dirs"]:
        flags.extend(["-I", os.path.join(c_dir, d)])
    for d in mod["wasm_defines"]:
        flags.append("-D" + d)
    return flags

def extract_cflags(mod):
    return [
        "--target=wasm32", "-nostdlib", "-O3", "-ffunction-sections",
        "-fno-builtin", "-fno-exceptions", "-mbulk-memory", "-matomics",
        "-c",
    ] + inc_flags(mod)

def link_cflags(mod):
    return [
        "--target=wasm32-wasi", "-nostdlib", "-O3", "-fno-builtin", "-fno-exceptions",
        "-mbulk-memory", "--sysroot=" + sysroot, "-c",
    ] + inc_flags(mod)

for mod in manifest["modules"]:
    kind = mod["kind"]
    mid = mod["id"]
    print("build-runtime:", kind, mid)
    if kind == "extract":
        srcs = mod["wasm_c"]
        if len(srcs) != 1:
            print("build-runtime: extract module %s must have one wasm_c file" % mid, file=sys.stderr)
            failed = 1
            continue
        src = os.path.join(c_dir, srcs[0])
        obj = os.path.join(out_dir, mid + ".o")
        err = os.path.join(out_dir, mid + ".clang.err")
        cmd = [clang] + extract_cflags(mod) + ["-o", obj, src]
        p = run(cmd, err)
        if p.returncode != 0:
            sys.stderr.write(open(err, encoding="utf-8").read())
            failed = 1
            continue
        try:
            printed = print_wat(obj)
        except SystemExit as e:
            print(e, file=sys.stderr)
            failed = 1
            continue
        rewritten = rewrite_imports(printed)
        open(os.path.join(out_dir, mid + ".full.wat"), "w", encoding="utf-8").write(rewritten)
        import re
        if re.search(forbidden, rewritten):
            print("build-runtime: extract gate failed for %s (stack pointer / memcpy / ctors)." % mid, file=sys.stderr)
            print("  Handwritten %s/%s stays the emit artifact." % (rt_dir, mod["wat_out"]), file=sys.stderr)
            failed = 1
            continue
        funcs = extract_funcs(rewritten)
        wat_path = os.path.join(out_dir, mid + ".wat")
        open(wat_path, "w", encoding="utf-8").write(funcs)
        if not funcs.strip():
            print("build-runtime: no funcs extracted from %s" % mid, file=sys.stderr)
            failed = 1
            continue
        if mod["promote"]:
            text = re.sub(r"\s*\(type \d+\)", "", funcs)
            text = text.replace("$strlen_dream", "$strlen")
            banner = ";; Generated by scripts/build-runtime.sh from runtime/c. Edit the .c, not this file.\n"
            dest = os.path.join(rt_dir, mod["wat_out"])
            body = banner + text
            if not body.endswith("\n"):
                body += "\n"
            open(dest, "w", encoding="utf-8").write(body)
            print("build-runtime: promoted", dest)
    elif kind == "link":
        objs = []
        cflags = link_cflags(mod)
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
        if failed or not os.path.isfile(wasm_ld):
            if not os.path.isfile(wasm_ld):
                print("build-runtime: wasm-ld not found at %s; skip %s link" % (wasm_ld, mid), file=sys.stderr)
                failed = 1
            continue
        wasm = os.path.join(out_dir, mid + ".wasm")
        cmd = [
            wasm_ld, "--no-entry", "--allow-undefined", "--import-memory",
        ]
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
        printed = print_wat(wasm)
        rewritten = rewrite_imports(printed)
        open(os.path.join(out_dir, mid + ".full.wat"), "w", encoding="utf-8").write(rewritten)
        if mod["promote"]:
            banner = ";; Generated by scripts/build-runtime.sh from runtime/c catalog. Edit the .c, not this file.\n"
            dest = os.path.join(rt_dir, mod["wat_out"])
            body = banner + rewritten
            if not body.endswith("\n"):
                body += "\n"
            open(dest, "w", encoding="utf-8").write(body)
            print("build-runtime: promoted", dest)
    else:
        print("build-runtime: unknown kind %s" % kind, file=sys.stderr)
        failed = 1

sys.exit(failed)
PY
status=$?
if [[ "$status" -ne 0 ]]; then
  if [[ "$CHECK" -eq 1 ]]; then
    die "clang/extract gates failed"
  fi
  die "some units failed"
fi

echo "build-runtime: wrote $OUT_DIR/*.wat and promoted catalog wat_out"
echo "  allocator.wat stays handwritten (debug/thread placeholders)."
if [[ "$CHECK" -eq 1 ]]; then
  echo "build-runtime --check: ok"
fi

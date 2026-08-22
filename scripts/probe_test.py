#!/usr/bin/env python3
"""Parallel golden-corpus probe for `tests/cases/*.dream`.

Default: `dream run` (native C).
`--node`: compile `--wasm` to `target/probe-wasm/{stem}/` and run via Node + `runtime/dream.js`.
"""
import json
import os
import re
import signal
import socket
import subprocess
import sys
import threading
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

_ANSI = re.compile(r"\x1b\[[0-9;]*m")

root = Path(__file__).resolve().parents[1]
dream = root / "target/debug/dream"
cases = sorted((root / "tests/cases").glob("*.dream"))
workers = int(os.environ.get("PROBE_JOBS", "8"))
dream_js = root / "runtime" / "dream.js"

USAGE = """\
Usage: probe_test.py [--node] [case-stem ...]

  --node     compile wasm32 C and run with Node (not native `dream run`)
  stems      optional filter (e.g. arithmetic webworker_basic)
"""

# Hosts that exist on native C only (files, sockets, GPU, interactive stdin).
_NODE_SKIP_PREFIXES = (
    "file_",
    "dir_",
    "http_",
    "tcp_",
    "net_",
    "ws_",
    "sqlite",
    "gpu_",
    # wgpu hosts: no WebGPU under Node.
    "render_",
    "compute_",
)
_NODE_SKIP_STEMS = {
    "console_read_line",
    "process_args_basic",
    "process_usage",
    # Asserts `System.platform() == Platform.Native`; true only on the native host.
    "platform_basic",
}


def parse_args(argv):
    only = []
    node = False
    for arg in argv:
        if arg in ("-h", "--help"):
            sys.stdout.write(USAGE)
            sys.exit(0)
        if arg == "--node":
            node = True
            continue
        if arg.startswith("-"):
            sys.stderr.write(f"unknown flag {arg}\n{USAGE}")
            sys.exit(2)
        only.append(arg)
    return node, (set(only) if only else None)


def run_group(args, timeout, stdin=None, env=None):
    proc_env = os.environ.copy()
    if env:
        proc_env.update(env)
    proc = subprocess.Popen(
        args,
        stdin=subprocess.PIPE if stdin is not None else None,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        start_new_session=True,
        cwd=root,
        env=proc_env,
    )
    try:
        out, err = proc.communicate(input=stdin, timeout=timeout)
        return proc.returncode, out or "", err or ""
    except subprocess.TimeoutExpired:
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        proc.wait()
        return -9, "timeout", ""


def run_output_body(out):
    # Program stdout only. Compiler diagnostics go to stderr; do not drop blank lines or
    # lines that happen to start with `error:` (e.g. `error: divide by zero`).
    return _ANSI.sub("", out).strip()


def spawn_http_mock():
    """Loopback HTTP mock for `http_methods_local` (same contract as e2e `DREAM_E2E_HTTP_PORT`).

    Echoes `METHOD path|x-tag|content-type|body` as the response body; `/bytes` serves a fixed
    binary payload. Handles sequential requests forever, like the Rust mock in e2e_tests.rs.
    """
    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind(("127.0.0.1", 0))
    listener.listen(8)
    port = listener.getsockname()[1]

    def serve(sock):
        try:
            while True:
                conn, _ = sock.accept()
                with conn:
                    while _serve_one(conn):
                        pass
        except OSError:
            pass
        finally:
            sock.close()

    def _serve_one(conn):
        data = b""
        while b"\r\n\r\n" not in data:
            chunk = conn.recv(4096)
            if not chunk:
                return False
            data += chunk
        head, _, rest = data.partition(b"\r\n\r\n")
        lines = head.split(b"\r\n")
        method, path, _ = lines[0].decode("latin1").split(" ", 2)
        length = 0
        x_tag = "-"
        content_type = "-"
        for line in lines[1:]:
            name, _, value = line.decode("latin1").partition(":")
            value = value.strip()
            if name.lower() == "content-length":
                length = int(value)
            elif name.lower() == "x-tag":
                x_tag = value
            elif name.lower() == "content-type":
                content_type = value
        while len(rest) < length:
            rest += conn.recv(4096)
        body = rest[:length].decode("utf-8", "replace")
        if path == "/bytes":
            payload = "bin-data-01"
        else:
            payload = f"{method} {path}|{x_tag}|{content_type}|{body}"
        if method == "HEAD":
            payload = ""
        response = (
            f"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n"
            f"Content-Length: {len(payload)}\r\nConnection: close\r\n\r\n{payload}"
        )
        conn.sendall(response.encode())
        return False

    thread = threading.Thread(target=serve, args=(listener,), daemon=True)
    thread.start()
    return port


def spawn_tcp_echo():
    """Loopback echo server for `tcp_echo_local` (same contract as e2e `DREAM_E2E_TCP_PORT`).

    The listener stays in the accept thread with no timeout: `dream run` compiles native C
    first, which can take longer than a short accept window when the probe is loaded.
    """
    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind(("127.0.0.1", 0))
    listener.listen(1)
    port = listener.getsockname()[1]

    def serve(sock):
        try:
            conn, _ = sock.accept()
            with conn:
                data = conn.recv(64)
                if data:
                    conn.sendall(data)
        except OSError:
            pass
        finally:
            sock.close()

    thread = threading.Thread(target=serve, args=(listener,), daemon=True)
    thread.start()
    return str(port)


def js_string(s):
    return json.dumps(s)


def file_url(p: Path) -> str:
    return p.resolve().as_uri()


def node_skip_reason(stem):
    if stem in _NODE_SKIP_STEMS:
        return "native-only host"
    for p in _NODE_SKIP_PREFIXES:
        if stem.startswith(p) or stem == p.rstrip("_"):
            return "native-only host"
    return None


def one_node(f: Path):
    stem = f.stem
    err = f.with_suffix(".expected_error")
    exp = f.with_suffix(".expected")
    trap = f.with_suffix(".expected_trap")
    dest_dir = root / "target" / "probe-wasm" / stem
    dest_dir.mkdir(parents=True, exist_ok=True)
    wat = dest_dir / f"{stem}.wat"
    compile_cmd = [
        str(dream),
        "--wasm",
        "-o",
        str(wat),
        str(f),
    ]
    if err.exists():
        code, _out, _err = run_group(compile_cmd, 60)
        if code == 0:
            return stem, "fail", "compile should fail"
        return stem, "ok", ""
    skip = node_skip_reason(stem)
    if skip:
        return stem, "skip", skip
    if trap.exists():
        return stem, "skip", "expected trap (native)"

    code, out, err_txt = run_group(compile_cmd, 180)
    if code != 0:
        tail = " | ".join((err_txt or out or "").strip().splitlines()[-2:])
        return stem, "fail", f"compile {code} {tail}"

    wasm = wat.with_suffix(".wasm")
    if not wasm.is_file():
        return stem, "fail", "missing .wasm"
    js_url = js_string(file_url(dream_js))
    wasm_url = js_string(str(wasm.resolve()))
    runner = dest_dir / f"{stem}_run.mjs"
    runner.write_text(
        f"""import {{ run }} from {js_url};
const chunks = [];
const timer = setTimeout(() => {{ console.error('probe --node timeout'); process.exit(2); }}, 25000);
await run({wasm_url}, {{ stdout: (s) => chunks.push(s) }});
clearTimeout(timer);
process.stdout.write(chunks.join(""));
"""
    )
    code, out, err_txt = run_group(["node", str(runner)], 35)
    if code != 0:
        tail = " | ".join((err_txt or out or "").strip().splitlines()[-2:])
        return stem, "fail", f"node {code} {tail}"
    if exp.exists():
        want = exp.read_text().strip()
        got = run_output_body(out)
        if got != want:
            return stem, "fail", f"output mismatch got={got[:80]!r}"
    return stem, "ok", ""


def one(f: Path):
    stem = f.stem
    err = f.with_suffix(".expected_error")
    exp = f.with_suffix(".expected")
    trap = f.with_suffix(".expected_trap")
    if err.exists():
        code, _out, _err = run_group([str(dream), str(f)], 25)
        if code == 0:
            return stem, "fail", "compile should fail"
        return stem, "ok", ""

    cmd = [str(dream), "run", str(f)]
    stdin = None
    env = None
    if stem == "console_read_line":
        stdin = "hello-line\n"
    elif stem == "process_args_basic":
        cmd.extend(["--", "alpha", "beta"])
    elif stem == "tcp_echo_local":
        env = {"DREAM_E2E_TCP_PORT": spawn_tcp_echo()}
    elif stem == "http_methods_local":
        env = {"DREAM_E2E_HTTP_PORT": str(spawn_http_mock())}

    # Debug `cc -O0` of large `@json` units is slow; leave headroom for a cold
    # `libdream_rt.a` rebuild and a loaded machine.
    code, out, err = run_group(cmd, 180, stdin=stdin, env=env)

    if trap.exists():
        if code == 0:
            return stem, "fail", "expected trap"
        return stem, "ok", ""
    if code != 0:
        tail = " | ".join((err or out or "").strip().splitlines()[-2:])
        return stem, "fail", f"run {code} {tail}"
    if exp.exists():
        want = exp.read_text().strip()
        got = run_output_body(out)
        if got != want:
            return stem, "fail", f"output mismatch got={got[:80]!r}"
    return stem, "ok", ""


def main():
    node, only = parse_args(sys.argv[1:])
    if not dream.is_file():
        sys.stderr.write(f"missing {dream}; build with `cargo build`\n")
        sys.exit(2)
    files = [p for p in cases if not only or p.stem in only]
    fails = []
    ok = 0
    skipped = 0
    run_one = one_node if node else one
    with ThreadPoolExecutor(max_workers=workers) as ex:
        futs = {ex.submit(run_one, p): p for p in files}
        done = 0
        for fut in as_completed(futs):
            done += 1
            label, status, msg = fut.result()
            extra = f" {msg}" if status == "skip" and msg else ""
            print(f"[{done}/{len(files)}] {label} {status}{extra}", flush=True)
            if status == "ok":
                ok += 1
            elif status == "skip":
                skipped += 1
            else:
                fails.append(f"{label}: {msg}")

    print(
        f"ok={ok} skip={skipped} fail={len(fails)} total={len(files)}",
        flush=True,
    )
    for line in sorted(fails):
        print(line)
    sys.exit(1 if fails else 0)


if __name__ == "__main__":
    main()

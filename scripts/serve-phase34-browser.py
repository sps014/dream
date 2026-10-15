#!/usr/bin/env python3
"""Compile and serve repeatable browser qualification fixtures for stdlib I/O, networking, cancellation, CLI, structured concurrency, channel, application utility, and compression APIs."""
import argparse
import functools
import http.server
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release", action="store_true")
    parser.add_argument("--case", choices=["http", "io-url", "cancellation", "cancellation-worker", "cli", "structured", "channels-worker", "utilities"])
    parser.add_argument("--port", type=int, default=60022)
    parser.add_argument("--dream", type=Path, default=ROOT / "target/debug/dream")
    args = parser.parse_args()
    output = ROOT / "target/phase34-browser"
    output.mkdir(parents=True, exist_ok=True)
    base = f"http://127.0.0.1:{args.port}"
    for name, fixture in [("http", "http"), ("io-url", "io_url"),
                          ("cancellation", "cancellation"), ("cancellation-worker", "cancellation_worker"), ("cli", "cli"), ("structured", "structured"), ("channels-worker", "channels_worker"), ("utilities", "utilities")]:
        if args.case and args.case != name:
            continue
        source = ROOT / f"tests/fixtures/compiler/stdlib_web_{fixture}.dream"
        target = output / f"{name}.dream"
        target.write_text(source.read_text().replace("__BASE__", base), encoding="utf-8")
        subprocess.run([str(args.dream.resolve()), *(["--release"] if args.release else []), "--runtime", "--web", str(target)], cwd=ROOT, check=True)
        (output / f"{name}.html").write_text(f'''<!doctype html><meta charset="utf-8">
<title>Dream stdlib: {name}</title><pre id="result">Running {name}…</pre>
<script type="module">
import {{ run }} from "./target/web/{name}.web.runtime.js";
try {{
  const instance = await run(new URL("./target/web/{name}.wasm", location.href));
  if (["structured", "channels-worker", "utilities"].includes("{name}") && instance.exports.__dream_leak_live?.()) throw new Error("live guest resources after completion");
  document.querySelector("#result").textContent = instance.exitCode ? `FAIL: ${{instance.exitCode}}` : "PASS: {name}";
}} catch (error) {{ document.querySelector("#result").textContent = `FAIL: ${{error.stack || error}}`; }}
</script>''', encoding="utf-8")
        print(f"{base}/target/phase34-browser/{name}.html", flush=True)

    class Handler(http.server.SimpleHTTPRequestHandler):
        def end_headers(self):
            self.send_header("Cross-Origin-Opener-Policy", "same-origin")
            self.send_header("Cross-Origin-Embedder-Policy", "require-corp")
            super().end_headers()

        def do_GET(self):
            if self.path == "/cors-redirect":
                self.send_response(302)
                self.send_header("Location", f"http://localhost:{args.port}/http")
                self.send_header("Content-Length", "0")
                self.end_headers()
            elif self.path in ["/http", "/slow", "/disconnect"]:
                if self.path == "/slow":
                    time.sleep(0.2)
                data = b"hello browser HTTP"
                self.send_response(200)
                self.send_header("Content-Type", "text/plain; charset=utf-8")
                self.send_header("Content-Length", "100" if self.path == "/disconnect" else str(len(data)))
                self.end_headers()
                try:
                    self.wfile.write(data)
                except (BrokenPipeError, ConnectionResetError):
                    pass
                if self.path == "/disconnect":
                    self.close_connection = True
            else:
                super().do_GET()

    handler = functools.partial(Handler, directory=str(ROOT))
    http.server.ThreadingHTTPServer(("127.0.0.1", args.port), handler).serve_forever()


if __name__ == "__main__":
    main()

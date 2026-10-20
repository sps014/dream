#!/usr/bin/env python3
"""Run compiled Dream compute/render fixtures in a real headless Chromium browser."""
import argparse
import http.server
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, default=ROOT / "target/minimal-wasm-size")
    parser.add_argument("--browser", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    browser = args.browser or next((Path(path) for path in [
        os.environ.get("DREAM_BROWSER", ""),
        "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
        shutil.which("chromium") or "", shutil.which("google-chrome") or "",
    ] if path and Path(path).is_file()), None)
    if not browser:
        raise SystemExit("set DREAM_BROWSER to a Chromium browser executable")
    complete = threading.Event()
    result = {}
    page = r"""<!doctype html><meta charset=utf-8><pre id=result>running</pre>
<script type=module>
let result;
try {
  if (!navigator.gpu) throw new Error('WebGPU is unavailable');
  await fetch('/progress?adapter');
  const adapter = await navigator.gpu.requestAdapter();
  if (!adapter) throw new Error('no WebGPU adapter');
  const outputs = {};
  for (const name of ['gpu_compute', 'gpu_render']) {
    await fetch('/progress?' + name);
    const {run} = await import('/' + name + '.web.runtime.js');
    let text = '';
    const instance = await run('/' + name + '.wasm', {stdout: value => text += value});
    if (instance.exitCode) throw new Error(name + ' failed: ' + instance.exitCode);
    outputs[name] = text.trim();
  }
  if (outputs.gpu_compute !== '6') throw new Error('compute readback: ' + outputs.gpu_compute);
  if (outputs.gpu_render !== '255\n51\n0\n255\n255') throw new Error('render readback: ' + outputs.gpu_render);
  result = {ok: true, adapter: adapter.info, outputs};
} catch (error) { result = {ok: false, error: String(error), stack: error.stack}; }
document.querySelector('#result').textContent = JSON.stringify(result);
await fetch('/complete', {method: 'POST', body: JSON.stringify(result)});
</script>"""

    class Handler(http.server.SimpleHTTPRequestHandler):
        def __init__(self, *values, **kwargs):
            super().__init__(*values, directory=str(args.directory), **kwargs)

        def do_GET(self):
            if self.path == "/":
                data = page.encode()
                self.send_response(200)
                self.send_header("Content-Type", "text/html; charset=utf-8")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)
            else:
                super().do_GET()

        def do_POST(self):
            if self.path != "/complete":
                self.send_error(404)
                return
            result.update(json.loads(self.rfile.read(int(self.headers["Content-Length"]))))
            self.send_response(204)
            self.end_headers()
            complete.set()

        def log_message(self, format, *values):
            print(format % values, flush=True)

    with http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler) as server, tempfile.TemporaryDirectory(prefix="dream-webgpu-") as profile:
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        command = [str(browser), "--headless=new", "--no-first-run", "--no-default-browser-check", "--disable-extensions",
                   "--remote-debugging-port=0", "--enable-unsafe-webgpu", "--ignore-gpu-blocklist", "--use-angle=d3d11" if os.name == "nt" else "--use-angle=swiftshader",
                   "--user-data-dir=" + profile, f"http://127.0.0.1:{server.server_port}/"]
        process = subprocess.Popen(command, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                   creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
        print("Browser profile: " + profile, flush=True)
        try:
            if not complete.wait(180):
                raise RuntimeError("browser qualification timed out")
        finally:
            if os.name == "nt":
                subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
            else:
                process.terminate()
            process.wait(timeout=15)
            server.shutdown()
    if args.output:
        args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8", newline="\n")
    print(json.dumps(result, indent=2))
    return 0 if result.get("ok") else 1


if __name__ == "__main__":
    raise SystemExit(main())

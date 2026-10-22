#!/usr/bin/env python3
"""Run a compiled Dream audio playback fixture in a real headless Chromium browser."""
import argparse
import http.server
import json
import math
import os
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile
import threading
import wave

ROOT = Path(__file__).resolve().parents[1]


def compile_stream(args, base):
    with wave.open(str(args.directory / "tone.wav"), "wb") as tone:
        tone.setnchannels(1)
        tone.setsampwidth(2)
        tone.setframerate(48000)
        period = b"".join(struct.pack("<h", round(16383 * math.sin(2 * math.pi * 440 * i / 48000)))
                           for i in range(48000))
        tone.writeframes(period * 120)
    source = args.directory / "audio-stream.dream"
    text = (ROOT / "tests/fixtures/compiler/audio_stream_browser.dream").read_text(encoding="utf-8")
    source.write_text(text.replace("__BASE__", base), encoding="utf-8")
    subprocess.run([str(args.compiler.resolve()), "--web", str(source),
                    "-o", str(args.directory / "audio-stream.wasm")], cwd=ROOT, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, default=ROOT / "target/target/web")
    parser.add_argument("--browser", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--compiler", type=Path, default=ROOT / "target/debug/dream.exe")
    args = parser.parse_args()
    args.directory.mkdir(parents=True, exist_ok=True)
    subprocess.run([str(args.compiler.resolve()), "--web",
                    str(ROOT / "tests/fixtures/compiler/audio_playback.dream"),
                    "-o", str(args.directory / "audio-playback.wasm")], cwd=ROOT, check=True)
    browser = args.browser or next((Path(path) for path in [
        os.environ.get("DREAM_BROWSER", ""),
        "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
        shutil.which("chromium") or "", shutil.which("google-chrome") or "",
    ] if path and Path(path).is_file()), None)
    if not browser:
        raise SystemExit("set DREAM_BROWSER to a Chromium browser executable")
    complete = threading.Event()
    result = {}
    media_ranges = []
    page = r"""<!doctype html><meta charset=utf-8><pre id=result>running</pre>
<script type=module>
let result;
try {
  const {run} = await import('/audio-playback.web.runtime.js');
  let output = '';
  const instance = await run('/audio-playback.wasm', {stdout: value => output += value});
  if (instance.exitCode) throw new Error('audio runner exited: ' + instance.exitCode);
  for (const check of ['decode: true', 'capture: true', 'sink: true', 'mute: true', 'pause: true', 'ended: true', 'restart: true']) {
    if (!output.includes(check)) throw new Error('playback check failed: ' + output);
  }
  const stream = await import('/audio-stream.web.runtime.js');
  const streamed = await stream.run('/audio-stream.wasm', {stdout: value => output += value});
  if (streamed.exitCode) throw new Error('audio stream runner exited: ' + streamed.exitCode);
  for (const check of ['stream progress: true', 'stream capture: true', 'stream seek: true']) {
    if (!output.includes(check)) throw new Error('stream check failed: ' + output);
  }
  result = {ok: true, output};
} catch (error) { result = {ok: false, error: String(error), stack: error.stack}; }
document.querySelector('#result').textContent = JSON.stringify(result);
await fetch('/complete', {method: 'POST', body: JSON.stringify(result)});
</script>"""

    class Handler(http.server.SimpleHTTPRequestHandler):
        def __init__(self, *values, **kwargs):
            super().__init__(*values, directory=str(args.directory), **kwargs)

        def do_GET(self):
            if self.path == "/tone.wav":
                data = (args.directory / "tone.wav").read_bytes()
                length = len(data)
                start, end = 0, length - 1
                request = self.headers.get("Range")
                if request:
                    left, right = request.removeprefix("bytes=").split("-", 1)
                    start = int(left or "0")
                    end = min(int(right) if right else end, end)
                media_ranges.append(start)
                # Cap each response so the far seek cannot be satisfied from a full download.
                end = min(end, start + 524287)
                self.send_response(206 if request else 200)
                self.send_header("Content-Type", "audio/wav")
                self.send_header("Accept-Ranges", "bytes")
                self.send_header("Content-Length", str(end - start + 1))
                if request:
                    self.send_header("Content-Range", f"bytes {start}-{end}/{length}")
                self.end_headers()
                self.wfile.write(data[start:end + 1])
            elif self.path == "/":
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

    with http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler) as server, tempfile.TemporaryDirectory(prefix="dream-audio-") as profile:
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        compile_stream(args, f"http://127.0.0.1:{server.server_port}")
        command = [str(browser), "--headless=new", "--no-first-run", "--no-default-browser-check", "--disable-extensions",
                   "--remote-debugging-port=0", "--autoplay-policy=no-user-gesture-required", "--ignore-gpu-blocklist", "--use-angle=d3d11" if os.name == "nt" else "--use-angle=swiftshader",
                   "--user-data-dir=" + profile, f"http://127.0.0.1:{server.server_port}/"]
        process = subprocess.Popen(command, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                   creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
        print("Browser profile: " + profile, flush=True)
        try:
            if not complete.wait(45):
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
    if result.get("ok") and not any(start > 48000 * 2 * 50 for start in media_ranges):
        raise RuntimeError(f"browser did not request a distant media range: {media_ranges}")
    return 0 if result.get("ok") else 1


if __name__ == "__main__":
    raise SystemExit(main())

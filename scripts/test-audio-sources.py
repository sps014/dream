#!/usr/bin/env python3
"""Qualify streamed audio I/O, detection, seeking and cancellation on native and Node."""
import argparse
import base64
import http.server
import math
from pathlib import Path
import struct
import subprocess
import threading
import time

ROOT = Path(__file__).resolve().parents[1]
WAV = base64.b64decode("UklGRiwAAABXQVZFZm10IBAAAAABAAEAgLsAAAB3AQACABAAZGF0YQgAAAAAAAAgAMD/fw==")


def tone_wav(seconds, rate=48000, channels=2, frequency=440.0):
    frames = int(seconds * rate)
    data = bytearray()
    for frame in range(frames):
        sample = int(8192 * math.sin(2 * math.pi * frequency * frame / rate))
        data += struct.pack("<h", sample) * channels
    header = b"RIFF" + struct.pack("<I", 36 + len(data)) + b"WAVEfmt "
    header += struct.pack("<IHHIIHH", 16, 1, channels, rate, rate * channels * 2, channels * 2, 16)
    return header + b"data" + struct.pack("<I", len(data)) + bytes(data)


def tagged_mp3(tag_bytes, frames):
    size = tag_bytes
    syncsafe = bytes([(size >> 21) & 0x7F, (size >> 14) & 0x7F, (size >> 7) & 0x7F, size & 0x7F])
    tag = b"ID3\x03\x00\x00" + syncsafe + bytes(size)
    return tag + (bytes.fromhex("fffb9064") + bytes(413)) * frames


LONG = tone_wav(10.0)
SHORT = tone_wav(0.3)
STALL = tone_wav(3.0)
MP3 = tagged_mp3(1_000_000, 2000)


def checked(command, **kwargs):
    process = subprocess.run(command, capture_output=True, text=True, timeout=300, **kwargs)
    if process.returncode:
        raise RuntimeError(f"{command[0]} exited {process.returncode}\n{process.stdout}{process.stderr}")
    return process.stdout + process.stderr


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", type=Path, default=ROOT / "target/debug/dream.exe")
    args = parser.parse_args()
    directory = ROOT / "target/audio-sources"
    directory.mkdir(parents=True, exist_ok=True)
    disguised = directory / "actually-wav.webp"
    disguised.write_bytes(WAV)

    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            bodies = {"/long.wav": LONG, "/short.wav": SHORT, "/stall.wav": STALL, "/tagged.mp3": MP3}
            data = bodies.get(self.path, b"bad" if self.path in ("/malformed", "/status") else WAV)
            if self.path == "/slow":
                time.sleep(0.3)
            self.send_response(503 if self.path == "/status" else 200)
            self.send_header("Content-Type", "audio/mpeg")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            try:
                if self.path in bodies:
                    # Stall after 1.5 s of audio, past the decoder lookahead and initial prefill, so playback underruns.
                    pause_at = 44 + 48000 * 4 * 3 // 2 if self.path == "/stall.wav" else -1
                    for start in range(0, len(data), 4096):
                        if start <= pause_at < start + 4096:
                            time.sleep(1.5)
                        self.wfile.write(data[start:start + 4096])
                else:
                    for byte in data:
                        self.wfile.write(bytes([byte]))
                        self.wfile.flush()
            except (BrokenPipeError, ConnectionResetError, ConnectionAbortedError):
                pass

        def log_message(self, *_):
            pass

    compiler = str(args.compiler.resolve())
    fixture = str(ROOT / "tests/fixtures/compiler/audio_sources.dream")
    with http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler) as server:
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        base = f"http://127.0.0.1:{server.server_port}"
        try:
            checked([compiler, "--relocatable", fixture, "-o", str(directory / "main.ll")], cwd=ROOT)
            output = checked([str(directory / "main.bin"), base, str(disguised), "playback"], cwd=ROOT)
            assert "audio sources: passed" in output, output
            assert "audio playback: passed" in output, output
            print("native sources and playback: passed", flush=True)
            checked([compiler, "--node", fixture, "-o", str(directory / "main.wasm")], cwd=ROOT)
            runner = directory / "runner.mjs"
            runner.write_text("import {run} from './main.node.runtime.js';\n"
                              "await run(new URL('./main.wasm', import.meta.url), {leakReport: true});\n",
                              encoding="utf-8")
            output = checked(["node", str(runner), base, str(disguised)], cwd=ROOT)
            assert "audio sources: passed" in output, output
            print("Node sources: passed", flush=True)
        finally:
            server.shutdown()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

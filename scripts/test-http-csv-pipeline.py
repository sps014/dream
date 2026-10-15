#!/usr/bin/env python3
"""Exercise the HTTP-to-CSV application on native and Node, including failure-safe output replacement."""
import argparse
import csv
import http.server
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading
import uuid

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release", action="store_true")
    parser.add_argument("--dream", type=Path, default=ROOT / "target/debug/dream")
    args = parser.parse_args()
    source = ROOT / "sample/http_csv_pipeline/main.dream"
    flags = ["--release"] if args.release else []
    subprocess.run([str(args.dream), *flags, str(source)], cwd=ROOT, check=True)
    subprocess.run([str(args.dream), *flags, "--runtime", "--node", str(source)], cwd=ROOT, check=True)
    target = source.parent / "target"
    native = target / ("release" if args.release else "debug") / ("main.exe" if os.name == "nt" else "main.bin")
    if not native.exists():
        native = native.with_suffix(".bin")
    records = [["Ada, Lovelace", "line\nnext"], ["こんにちは", 'say "hi"']]
    buffer = io.StringIO(newline="")
    csv.writer(buffer).writerows(records)
    valid = buffer.getvalue().encode()

    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            data = valid if self.path == "/valid" else b'"unterminated'
            self.send_response(200)
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def log_message(self, *_args):
            pass

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        with tempfile.TemporaryDirectory(prefix="dream-http-csv-") as temporary:
            directory = Path(temporary)
            runner = directory / "run.mjs"
            runner.write_text(
                "import {readFile} from 'node:fs/promises';\n"
                + "const {run}=await import(" + json.dumps((target / "web/main.node.runtime.js").as_uri()) + ");\n"
                + "const instance=await run(await readFile(" + json.dumps(str(target / "web/main.wasm")) + "));\n"
                + "if(instance.exports.__dream_leak_live?.()) throw new Error('guest resources leaked');\n"
                + "process.exitCode=instance.exitCode;\n", encoding="utf-8")
            for host, command in [("native", [str(native)]), ("node", ["node", str(runner)])]:
                output = directory / f"{host}.csv"
                url = f"http://127.0.0.1:{server.server_port}"
                result = subprocess.run([*command, url + "/valid", "--output", str(output)], cwd=directory, capture_output=True, text=True, timeout=60)
                if result.returncode:
                    raise RuntimeError(f"{host}: {result.stdout}\n{result.stderr}")
                with output.open(newline="", encoding="utf-8") as stream:
                    actual = list(csv.reader(stream))
                assert [row[1:] for row in actual] == records, actual
                assert all(uuid.UUID(row[0]).version == 4 for row in actual)
                output.write_text("original", encoding="utf-8")
                result = subprocess.run([*command, url + "/invalid", "--output", str(output)], cwd=directory, capture_output=True, text=True, timeout=60)
                assert result.returncode == 1, (host, result.stdout, result.stderr)
                assert output.read_text(encoding="utf-8") == "original"
                print(f"PASS {host}: CSV values, UUIDs, malformed input, atomic replacement", flush=True)
    finally:
        server.shutdown()
        server.server_close()
        thread.join()


if __name__ == "__main__":
    main()

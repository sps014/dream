#!/usr/bin/env python3
"""Qualify standalone WASI command components with sandboxed files and real HTTP."""
import argparse
from contextlib import ExitStack
import http.server
import os
from pathlib import Path
import subprocess
import socketserver
import tempfile
import threading

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dream", type=Path, default=ROOT / "target/debug" / ("dream.exe" if os.name == "nt" else "dream"))
    parser.add_argument("--wasmtime", type=Path, default=Path.home() / ".dream/toolchains/wasmtime-49.0.4" / ("wasmtime.exe" if os.name == "nt" else "wasmtime"))
    args = parser.parse_args()

    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            data = "component network 日本語".encode()
            self.send_response(200)
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def log_message(self, *_):
            pass

    class Echo(socketserver.BaseRequestHandler):
        def handle(self):
            data = self.request.recv(1024)
            self.request.sendall(data)

    with socketserver.ThreadingTCPServer(("127.0.0.1", 0), Echo) as tcp, ExitStack() as cleanup:
        cleanup.callback(tcp.shutdown)
        threading.Thread(target=tcp.serve_forever, daemon=True).start()
        qualify(args, Handler, tcp.server_address[1])


def qualify(args, handler, tcp_port):
    with http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler) as server, ExitStack() as cleanup, tempfile.TemporaryDirectory(prefix="dream-wasi-") as temporary:
        cleanup.callback(server.shutdown)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        directory = Path(temporary)
        source = directory / "check.dream"
        source.write_text('''import system;
import system.io;
import system.net;
import system.interop;

@c("c", "abs")
extern fun c_abs(value: int): int;

async fun main(): void {
    let written = File.write("component-test.txt", "portable filesystem").await;
    if written.is_err() {
        System.println("filesystem denied");
        return;
    }
    Time.sleep(1).await;
    System.println(c_abs(0 - 7));
    System.println(File.read("component-test.txt").await.unwrap());
    let client = HttpClient();
    let request = HttpRequest(HttpMethod.Get, Url.parse("http://127.0.0.1:PORT/check?q=value").unwrap());
    let response = client.send(request).await.unwrap();
    System.println(response.status);
    System.println(response.read_text().await.unwrap());
    response.close();
    client.close();
    let tcp = TcpStream.connect("127.0.0.1", TCP_PORT, 1000).await.unwrap();
    tcp.write_all_async(Encoding.utf8_encode("tcp echo")).await.unwrap();
    System.println(Encoding.utf8_decode(tcp.read_with_timeout(8, 1000).await.unwrap()));
    tcp.close();
    let udp = UdpSocket.bind("127.0.0.1", 0).await.unwrap();
    let sent = udp.send("127.0.0.1", udp.local_port, Encoding.utf8_encode("udp echo"), 1000).await;
    if sent.is_err() {
        System.println(sent.unwrap_err().message());
        return;
    }
    System.println(Encoding.utf8_decode(udp.receive(8, 1000).await.unwrap().data));
    udp.close();
}
'''.replace("TCP_PORT", str(tcp_port)).replace("PORT", str(server.server_port)), encoding="utf-8", newline="\n")
        for preview in ["p2", "p3"]:
            output = directory / ("check_" + preview + ".wat")
            subprocess.run([str(args.dream.resolve()), "--target", "wasm32-wasi" + preview, "-o", str(output), str(source)], cwd=ROOT, check=True, timeout=180)
            artifacts = [output, output.with_suffix(".wasm"), output.with_suffix(".abi.json")]
            previous = [path.read_bytes() for path in artifacts]
            output.with_suffix(".wasm").unlink()
            subprocess.run([str(args.dream.resolve()), "--target", "wasm32-wasi" + preview, "-o", str(output), str(source)], cwd=ROOT, check=True, timeout=180)
            if previous != [path.read_bytes() for path in artifacts]:
                raise RuntimeError(preview + " component output is not deterministic")
            flags = [str(args.wasmtime.resolve()), "run", "-S", "http=y", "-S", "inherit-network=y"]
            if preview == "p3":
                flags += ["-S", "p3=y", "-W", "component-model-async=y", "-W", "component-model-more-async-builtins=y", "-W", "component-model-async-stackful=y"]
            command = flags + ["--dir", str(directory) + "::.", str(output.with_suffix(".wasm"))]
            run = subprocess.run(command, check=False, capture_output=True, text=True, encoding="utf-8", timeout=45)
            if run.returncode:
                raise RuntimeError(preview + " execution failed: " + run.stdout + run.stderr)
            if run.stdout.replace("\r\n", "\n") != "7\nportable filesystem\n200\ncomponent network 日本語\ntcp echo\nudp echo\n":
                raise RuntimeError(preview + " output: " + repr(run.stdout) + run.stderr)
            denied = subprocess.run(flags + [str(output.with_suffix(".wasm"))], check=True, capture_output=True, text=True, encoding="utf-8", timeout=45)
            if denied.stdout.strip() != "filesystem denied":
                raise RuntimeError(preview + " failed to enforce filesystem sandbox: " + repr(denied.stdout))
            print("WASI" + preview + ": C ABI, timer, filesystem, HTTP, TCP, UDP, UTF-8, body EOF, and filesystem sandbox passed.", flush=True)


if __name__ == "__main__":
    main()

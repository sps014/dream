# `system.webview` samples

Native-only (`dream run`). Works with the public installer on macOS, Windows, and Linux (the installer pulls WebKitGTK on Linux). Needs a graphical session; in Docker pass a display (`-e DISPLAY` or Xvfb).

```bash
# from repo root
cargo run -- run sample/webview/hello.dream
cargo run -- run sample/webview/ipc.dream

# or from this directory
dream run hello.dream
dream run ipc.dream
```

- `hello.dream` — open a URL and wait until the window closes
- `ipc.dream` — typed IPC counter with `ui/index.html` (path resolves from repo root or this folder)

The host waits for page navigation to commit before `load_*` returns so `emit`/`reply` are not stuck in wry's pre-load script queue.

# `system.webview` samples

Native-only (`dream run`).

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

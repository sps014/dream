# Quickstart

From a blank machine to a running program in a few minutes.

<div class="dream-steps" markdown>

1. **Install Dream** with the command for your OS (below).
2. **Open a new terminal** so `dream` and `dreamer` are on your PATH.
3. **Create and run** a project with `dreamer init hello`.

</div>

## Install

### macOS / Linux

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sps014.github.io/dream/install.sh | sh
```

### Windows (PowerShell)

```powershell
irm https://sps014.github.io/dream/install.ps1 | iex
```

That installs `dream` (compile and run), `dreamer` (projects and packages), and `dream-lsp` (editor support) under `~/.dream/bin`. If no C compiler (`cc` / `clang` / Zig) is already on the machine, the installer also runs `dreamer toolchain install cc` (pinned Zig). Set `DREAM_SKIP_CC=1` to skip that download. On Linux it also installs WebKitGTK/GTK when `dream` cannot load (needed for `system.webview`); set `DREAM_SKIP_LIBS=1` to skip. Linux binaries need glibc 2.36+ (Debian 12, Ubuntu 24.04, Fedora 39, or newer).

Open a **new terminal**, then check:

```bash
dream --help
dreamer --help
```

Pin a version with `DREAM_VERSION=0.0.1` before running the installer.

## Hello, World!

```bash
dreamer init hello
cd hello
dreamer run
```

`dream run` with no file argument uses `[package].entry` from the nearest `dream.toml`. Prefer `dreamer run` for packages (dependencies and web/node hosts).

`dreamer init` creates `dream.toml`, `src/main.dream`, and a `.gitignore`. Edit `src/main.dream`:

```dream
import system;

fun main() {
    System.println("Hello, world!");
}
```

```
Hello, world!
```

- `fun main()` — the program starts here.
- `System.println(...)` — print a line, then a newline (`import system;`).

Without a project folder you can still run a single file:

```bash
dream run hello.dream
```

Compile without running (writes a WebAssembly module under `target/web/`):

```bash
dream hello.dream
```

## Going further

### Browser or Node

```bash
dreamer init hello --runtime web,node && cd hello
dreamer run --target web
dreamer run --target node
```

Or with `dream`:

```bash
dream --runtime --web hello.dream
dream --runtime --node hello.dream
```

```javascript
import { run } from "./target/web/hello.web.runtime.js";
await run("target/web/hello.wasm");
```

See [JavaScript interop](../reference/language/interop.md).

## Next

- [Language tour](tour.md)
- [Cookbook](../cookbook/index.md)
- [Package manager](../reference/tooling/dreamer.md)

# Dream

Dream is a blazing fast, typed programming language with familiar `fun` / `let` syntax. You write one program; it compiles to WebAssembly and can run on your computer, in the browser, or in Node. Memory is automatic reference counting (ARC) — no manual `free`. 

**[Docs](https://sps014.github.io/dream/)** · [Quickstart](https://sps014.github.io/dream/learn/quickstart/) · [Language tour](https://sps014.github.io/dream/learn/tour/) · [Cookbook](https://sps014.github.io/dream/cookbook/)

## 5-minute quickstart

**macOS / Linux:**

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sps014.github.io/dream/install.sh | sh
```

**Windows (PowerShell):**

```powershell
irm https://sps014.github.io/dream/install.ps1 | iex
```

Open a new terminal, then:

```bash
dreamer init hello && cd hello && dreamer run
```

That creates a project and runs `src/main.dream`:

```kotlin
import system;

fun main() {
    System.println("Hello, world!");
}
```

```
Hello, world!
```

`import system;` loads console I/O so `System.println` works. Full walkthrough: [Quickstart](https://sps014.github.io/dream/learn/quickstart/).

## Language tour

### Variables

```kotlin
import system;

fun main() {
    let name = "Ada";     // inferred as string; you can change it later
    const n = 3;          // a number that cannot be reassigned
    System.println(name);
    System.println(n);
}
```

### Control flow

```kotlin
import system;

fun main() {
    let score = 85;
    if (score >= 90) {              // conditions go in parentheses
        System.println("A");
    } else {
        System.println("B");
    }

    let i = 0;
    while (i < 3) {                 // repeat while the condition is true
        System.println(i);
        i = i + 1;
    }
}
```

### Functions

```kotlin
import system;

fun greet(name: string): string {   // name in, string out
    return "Hello, " + name;
}

fun main() {
    System.println(greet("world"));
}
```

### Lists

```kotlin
import system;
import system.collections;

fun main() {
    let xs = List<int>();           // a growable list of integers
    xs.push(1);
    xs.push(2);

    for (let n in xs) {             // n is each element in turn
        System.println(n);
    }
}
```

More syntax: [Language tour](https://sps014.github.io/dream/learn/tour/).

## Docs

| Section | What it is |
| --- | --- |
| [Learn](https://sps014.github.io/dream/learn/) | Install, Hello World, and a short tour |
| [Reference](https://sps014.github.io/dream/reference/language/variables/) | Language, stdlib, and `dreamer` |
| [Cookbook](https://sps014.github.io/dream/cookbook/) | Small, copy-paste programs |
| [Internals](https://sps014.github.io/dream/internals/) | Compiler handbook (contributors) |

## Next steps

- [Quickstart](https://sps014.github.io/dream/learn/quickstart/) — install and run
- [Standard library](https://sps014.github.io/dream/reference/stdlib/) — collections, files, HTTP, JSON, GPU, crypto, and more
- [Package manager](https://sps014.github.io/dream/reference/tooling/dreamer/) — `dreamer` projects and packages
- [Cookbook](https://sps014.github.io/dream/cookbook/) — more small examples

**Community:** [GitHub Issues](https://github.com/sps014/dream/issues) · Discussions (coming soon) · Discord (coming soon)

## Contributors

**Prerequisites**

| You want | Need |
| --- | --- |
| Installer / `dreamer run` | Nothing besides the [install script](#5-minute-quickstart) (no Rust) |
| Build and test the compiler | [Rust](https://rustup.rs/) (stable `rustc` + `cargo`) |
| `dream run --backend c` / native-C e2e | `dreamer toolchain install cc` (Zig) or a clang-compatible `CC` on `PATH` |
| Rebuild guest `regex.wat` | `dreamer toolchain install wasi-sdk` and `wasm-tools` or `wasm2wat` — [runtime README](crates/dream-mir/src/runtime/README.md). Not used by `cargo test` or Windows CI |
| JS runtime bundle | Node.js (`node scripts/bundle-runtime.mjs`) |
| Docs site | Python 3; `python3 -m venv .venv && .venv/bin/pip install -r docs/requirements-docs.txt` then `mkdocs build --strict` |

```bash
git clone https://github.com/sps014/dream
cd dream
source ./use-toolchain.sh   # builds release dream / dream-lsp / dreamer into ~/.dream/bin
```

```bash
cargo test --workspace                 # fast gate
cargo test --workspace -- --ignored    # full corpus, DAP, wasm-opt, native-C goldens
```

Compiler internals: [docs/internals](https://sps014.github.io/dream/internals/).

## License

MIT

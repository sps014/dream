# Dream

Dream is a typed programming language with familiar `fun` / `let` syntax. You write one program; it compiles to WebAssembly and can run on your computer, in the browser, or in Node. Memory is managed for you — you do not allocate or free by hand.

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

```bash
git clone https://github.com/sps014/dream
cd dream
source ./use-toolchain.sh
```

```bash
cargo test --workspace                 # fast gate
cargo test --workspace -- --ignored    # full corpus
```

Compiler internals: [docs/internals](https://sps014.github.io/dream/internals/).

## License

MIT

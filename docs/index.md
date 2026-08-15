---
hide:
  - navigation
  - toc
---

<div class="dream-hero">
  <h1 class="dream-gradient-text">Dream</h1>
  <p class="dream-hero-subtitle">
    A typed language with familiar <code>fun</code> / <code>let</code> syntax.
    Write once, compile to WebAssembly, and run on your computer, in the browser, or in Node.
    Memory is automatic reference counting (ARC).
  </p>
  <div class="dream-hero-actions">
    <a href="learn/quickstart/" class="md-button md-button--primary">Get started</a>
    <a href="learn/tour/" class="md-button">Language tour</a>
    <a href="https://github.com/sps014/dream" class="md-button">GitHub</a>
  </div>
</div>

<div class="dream-code-showcase" markdown>

```dream
import system;

fun main() {
    System.println("Hello, world!");
}
```

</div>

## What you get

<div class="dream-feature-strip" markdown>

- **Static types** with inference
- **ARC** memory
- **Generics** monomorphized to WASM
- **Classes / structs / interfaces**
- **Enums & unions** with `switch`
- **`async` / `await`** in-module
- **`WebWorker`** parallelism
- **`js` + `extern`** interop
- **`@compute`** → WebGPU
- **Stdlib** collections, JSON, I/O, GPU, crypto

</div>

## Start here

<div class="dream-compact-cards" markdown>

<div class="grid cards" markdown>

-   :material-rocket-launch: **Learn**

    ---

    Install Dream and write your first program in a few minutes.

    [:octicons-arrow-right-24: Quickstart](learn/quickstart.md)

-   :material-book-open-page-variant: **Language tour**

    ---

    Variables, `if` / loops, functions, and lists — with notes on each line.

    [:octicons-arrow-right-24: Tour](learn/tour.md)

-   :material-chef-hat: **Cookbook**

    ---

    Hello World, lists, GPU, and a source generator.

    [:octicons-arrow-right-24: Recipes](cookbook/index.md)

-   :material-book-search: **Reference**

    ---

    Language rules, standard library, and `dreamer`.

    [:octicons-arrow-right-24: Variables](reference/language/variables.md)

</div>

</div>

## Language

<div class="dream-compact-cards" markdown>

<div class="grid cards" markdown>

-   :material-variable: **Basics**

    ---

    [Variables](reference/language/variables.md) · [Operators](reference/language/operators.md) ·
    [Control flow](reference/language/control-flow.md) · [Functions](reference/language/functions.md) ·
    [Comments](reference/language/comments.md) · [Panics](reference/language/panics.md)

-   :material-cube: **Types**

    ---

    [Overview](reference/language/types.md) · [Primitives](reference/language/primitives.md) ·
    [Arrays](reference/language/arrays.md) · [Enums & unions](reference/language/enums-unions.md) ·
    [Classes & structs](reference/language/classes-structs.md) · [object](reference/language/objects.md)

-   :material-folder-outline: **Structure**

    ---

    [Imports](reference/language/imports.md) · [Language rules](reference/language/invariants.md)

-   :material-puzzle: **Features**

    ---

    [Generics](reference/language/generics.md) · [Interfaces](reference/language/interfaces.md) ·
    [Async](reference/language/async.md) · [WebWorkers](reference/language/webworkers.md) ·
    [Compute](reference/language/compute.md) · [Shaders](reference/language/shaders.md) ·
    [Memory](reference/language/memory.md)

-   :material-language-javascript: **Interop**

    ---

    [JavaScript](reference/language/interop.md) · [js type](reference/language/js-type.md) ·
    [Callbacks](reference/language/callbacks.md) · [C](reference/language/c-interop.md)

-   :material-auto-fix: **Metaprogramming**

    ---

    [Source generators](reference/language/generators.md) · [CodeBuilder](reference/stdlib/codegen.md)

</div>

</div>

## Standard library

<div class="dream-compact-cards" markdown>

<div class="grid cards" markdown>

-   :material-code-braces: **Core**

    ---

    [Built-ins](reference/stdlib/builtins.md) · [Option & Result](reference/stdlib/option-result.md) ·
    [Sync](reference/stdlib/sync.md)

-   :material-format-text: **Text**

    ---

    [Strings](reference/stdlib/string.md) · [Regex](reference/stdlib/regex.md) ·
    [Encoding](reference/stdlib/encoding.md)

-   :material-layers: **Collections**

    ---

    [List / Map / Set](reference/stdlib/collections.md)

-   :material-cog: **System**

    ---

    [Random](reference/stdlib/random.md) · [DateTime](reference/stdlib/datetime.md) ·
    [Logging](reference/stdlib/logging.md) · [Testing](reference/stdlib/testing.md) ·
    [Process](reference/stdlib/process.md) · [WebView](reference/stdlib/webview.md)

-   :material-swap-horizontal: **I/O**

    ---

    [File](reference/stdlib/file.md) · [HTTP](reference/stdlib/http.md) ·
    [Sockets](reference/stdlib/net.md)

-   :material-gpu: **GPU**

    ---

    [system.gpu](reference/stdlib/gpu.md) · [Compute](reference/language/compute.md)

-   :material-code-json: **JSON**

    ---

    [JSON & `@json`](reference/stdlib/json.md)

-   :material-shield-key: **Crypto**

    ---

    [Digests & CSPRNG](reference/stdlib/crypto.md)

</div>

</div>

<div class="dream-community" markdown>

**Community.** Questions and bugs: [GitHub Issues](https://github.com/sps014/dream/issues). Discussions and Discord: coming soon.

[:octicons-arrow-right-24: Next steps](learn/next-steps.md)

</div>

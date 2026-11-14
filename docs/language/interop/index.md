---
title: Interop
summary: "Import JavaScript modules and call them from Dream."
icon: material/swap-horizontal
---

# JavaScript interop

`jsimport` reads a module's declarations and bundles its implementation. It works with `--web` and `--node`. A native build rejects it.

```dream
import system;

jsimport "./math.mjs" as math;

fun main(): void {
    System.println(math.add(2, 3));
}
```

Place the module and its declarations beside `main.dream`:

```javascript title="math.mjs"
export function add(left, right) {
    return left + right;
}
```

```typescript title="math.d.mts"
export function add(left: number, right: number): number;
```

```sh
dream --runtime --node main.dream
```

Load `main.node.runtime.mjs` together with `main.node.jsimports.mjs`. A browser build uses `--web` and `main.web.runtime.js`.

## Packages

List packages in `dream.toml`. The first build installs them into `.dream/npm` and writes `dream.npm.lock`. Commit that lock file. Installation scripts do not run.

```toml
[npm]
escape-string-regexp = "5.0.0"
```

Import it with `jsimport "escape-string-regexp" as pattern;`. A default export is `pattern.default(...)`. Add an `@types` package to `[npm]` when a library ships no declarations.

A URL import uses the same form. Its downloaded graph is recorded in `dream.js.lock`.

```dream
jsimport "https://example.com/library.mjs" as library;
```

Omit the alias when the module only needs to run:

```dream
jsimport "./setup.mjs";

fun main(): void {}
```

`DREAM_NO_AUTO_INSTALL=1` skips downloads.

## Values

JavaScript numbers are `double`, booleans are `bool`, strings are `string`, and bigints are `long`. Nullable results are `Option`. An awaited promise is `Result<T, JsError>`. A type Dream cannot represent stays `js`.

`import system.js;` provides `console`, `JSON`, `URL`, and timers. Page objects such as `document` are `js`.

```dream
import system.js;

fun main(): void {
    let element = js.global.document.getElementById("app");
    if !element.is_null() {
        element.textContent = "Hello";
    }
}
```

See [The js type](js-type.md) and [Callbacks](callbacks.md).

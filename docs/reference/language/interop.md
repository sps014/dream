# JS Interop

Dream runs in the browser and Node as WebAssembly. Talking to JavaScript is built on three pieces, each with its own page:

| Piece | What it's for | Docs |
| --- | --- | --- |
| `extern fun` | a typed, fixed-signature function that lives in JS (`Math.max`, your glue code) | this page |
| `js` | a dynamic handle to *any* live JS value, used with native syntax | [The js type](js-type.md) |
| function values | passing functions across the boundary in either direction | [Callbacks](callbacks.md) |
| `@c(...)` | binds an extern to a native C library (`dream run` only) | [C Interop](c-interop.md) |

`Js.*` (`dream_js_call`) is WASM/JS-host only: native C aborts if guest code tries to call into JavaScript. `system.webview` is native-only.

This page covers `extern` functions.

## Declaring an extern function

An `extern fun` has a signature but no body. Call it like any other function:

```dream
extern fun alert(msg: string): void;

fun main(): void {
    alert("Hello from Dream!");
}
```

By default it binds to a JS global of the same name (`alert`). Three things cooperate:

- `extern fun` declares the signature on the Dream side.
- `@js("module", "field")` optionally remaps which JS object and property it binds to.
- The runtime marshals values, binds externs to JS globals, and bridges Promises for `extern async fun`.

!!! note "Restrictions"
    Extern functions cannot have a body, cannot be generic, and cannot be combined with `public`.

## Remapping the import name

`@js(module, name)` controls which JS object and property the extern binds to:

```dream
@js("dom", "setText")            // binds to importObject["dom"]["setText"]
extern fun set_text(value: string): void;

@js("console")                   // module only -> field defaults to the function name
extern fun log(msg: string): void;
```

## Running it from JavaScript

Compile a `.dream` file, then load the `.wasm` with the Dream runtime:

```javascript
import { run } from "./runtime/dream.js";
await run("hello.wasm");   // binds externs, calls main
```

Optionally emit a smaller sibling `*.web.runtime.js` / `*.node.runtime.js` (only the host pieces that program needs) with `--runtime` plus one or both host flags. Without these flags, compile does **not** write a selective runtime — use the full shared host instead.

```bash
dream --runtime --web hello.dream           # hello.web.runtime.js
dream --runtime --node hello.dream          # hello.node.runtime.js
dream --runtime --web --node hello.dream    # both in one compile
```

```javascript
import { run } from "./hello.web.runtime.js";
await run("hello.wasm");
```

### Auto-binding to JS globals

For every extern you do not supply explicitly, the runtime resolves it against the JS global scope:

- The default `env` module maps to a bare global — `extern fun alert(...)` binds to `alert`.
- `@js("module", "name")` maps to a property — `@js("console", "log")` binds to `console.log`, `@js("Math", "max")` to `Math.max`.

Built-in browser and Node APIs therefore need no glue. Pass `imports` only for your own functions, keyed by the Dream function name:

```javascript
await run("hello.wasm", {
  imports: {
    square: (n) => n * n,
  },
});
```

If an extern matches no global and you don't provide it, the runtime installs a stub that throws only if actually called — so the module still loads. For full control, use `load(source, options)` instead of `run`; it returns the instance without calling `main`.

## Value marshaling

Arguments and returns convert between Dream and JavaScript:

| Dream type | As argument | As return value |
|------------|-------------|-----------------|
| `int`, `float`, `double` | `number` | `number` |
| `bool` | `boolean` | `boolean` |
| `string` | `string` | return a `string` |
| `T[]` | `Array` of marshaled elements | (pointer) |
| `object`, classes, `List<T>` | opaque pointer (`number`) | (pointer) |

For reference types, read the underlying data with the instance helpers:

```javascript
mod.readString(ptr);          // UTF-16 string
mod.readArray(ptr, "int");    // -> number[]
mod.readList(ptr, "string");  // List<string> -> string[]
mod.readStruct(ptr, [         // class by field schema (declaration order)
  { name: "x", type: "int" },
  { name: "y", type: "int" },
]);
```

To hand a string back to Dream, call `mod.writeString(str)` (the runtime also does this for you when a return value is a `string`).

## Beyond fixed signatures

`extern fun` is ideal for known signatures. For open-ended JS values (a DOM node, a `fetch` `Response`, a `RegExp`) you want to read and call natively, use the dynamic [`js`](js-type.md) type:

```dream
let el = js.global.document.getElementById("app");
el.textContent = "hello";
```

Functions cross the boundary in both directions too — see [Callbacks](callbacks.md).

## Built on interop

- [Regex](../stdlib/regex.md) — `Regex` in `system.text` (not a JS `RegExp`).
- [HttpClient](../stdlib/http.md) — HTTP over `extern async fun`.

## Host feature matrix

Most stdlib packages behave identically on `dream run`, in the browser, and in Node. Where a host can't support a feature (no raw sockets in the browser, no process model in the browser, …) that package's own page has a "Platform notes" table — check
[HTTP](../stdlib/http.md), [Raw sockets](../stdlib/net.md), [File I/O](../stdlib/file.md),
[Process](../stdlib/process.md), [Crypto](../stdlib/crypto.md), [DateTime](../stdlib/datetime.md),
and [`system.gpu`](../stdlib/gpu.md). Unicode helpers ([Strings](../stdlib/string.md)) and `System.read_line` / `read_key` ([Built-ins](../stdlib/builtins.md)) work on both native and JS hosts; in the browser, interactive input uses a `prompt()` dialog because there is no stdin.

---
title: Interop
summary: "Call JavaScript with ordinary Dream syntax."
icon: material/swap-horizontal
---

# JavaScript interop

Use `js.global` to access browser or Node APIs. Read properties, call methods, and assign values
with ordinary Dream syntax. No binding declarations or imports are needed.

```dream
fun main(): void {
    let console = js.global.console;
    console.log("Hello from Dream!");
}
```

Compile for Node with `dream --node main.dream`, or for the browser with
`dream --web main.dream`. Load the output using the emitted runtime:

```javascript
import { run } from "./main.node.runtime.js";
await run("./main.wasm");
```

For a browser, import `main.web.runtime.js` instead and load it from a page served over HTTP.
If your program uses CSV or compression codecs, also deploy the generated
`main.data.wasm` beside the runtime file. The loader loads this companion automatically;
programs without those codecs do not emit it. Keep the companion when copying a build
to another directory or web server. A missing companion makes `load` or `run` reject,
so handle that Promise failure alongside other loading errors.
JavaScript APIs require the corresponding host: browser DOM APIs are unavailable in Node.

## Working with values

Dynamic operations return `js`. Assign a result to a typed variable to convert it to a Dream value.

```dream
fun main(): void {
    let text: string = js.global.JSON.stringify(js.object());
    System.println(text);
}
```

Member names are checked by JavaScript when the operation runs. A missing function, JavaScript
exception, or failed conversion is a runtime failure; use `try`/`catch` in a JavaScript function
when an external API needs recoverable exception handling, and return an explicit result to Dream.
For promises, use the async facilities described in [The js type](js-type.md).

Pass Dream functions directly as callbacks. See [Callbacks](callbacks.md) for callback identity
and supported signatures. [The js type](js-type.md) covers arrays, objects, conversion, and promises.

Explicit JS `extern` bindings are internal implementation facilities. Application-facing APIs use
`js` syntax or ordinary Dream functions with bodies. Compiler contributors can read
[Internal JavaScript bindings](../../internals/21-js-bindings.md).

Native library integration uses [C interop](c.md) or [C++ interop](cpp.md).

Use [Standalone WASI commands](wasi.md) to run command components in a WASI host.

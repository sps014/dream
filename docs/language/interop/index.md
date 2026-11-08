---
title: Interop
summary: "Call JavaScript with ordinary Dream syntax."
icon: material/swap-horizontal
---

# JavaScript interop

Use `js.global` to read, call, and assign browser or Node APIs with ordinary Dream syntax. No binding declarations or imports are needed.

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
JavaScript APIs require the corresponding host: browser DOM APIs are unavailable in Node.

## Working with values

Dynamic operations return `js`. Assign a result to a typed variable to convert it to a Dream value.

```dream
fun main(): void {
    let text: string = js.global.JSON.stringify(js.object());
    System.println(text);
}
```

Member names are checked when the JavaScript runs, and a missing function, exception, or failed conversion stops that operation. Catch a recoverable error in JavaScript and return a result; promises are covered in [The js type](js-type.md).

Pass Dream functions directly as callbacks. [Callbacks](callbacks.md) covers identity and signatures, and [The js type](js-type.md) covers arrays, objects, conversion, and promises.

Application code uses `js` syntax or ordinary Dream functions. Explicit JS `extern` bindings are for the compiler — see [Internal JavaScript bindings](../../internals/21-js-bindings.md).

Native library integration uses [C interop](c.md) or [C++ interop](cpp.md).

Use [Standalone WASI commands](wasi.md) to run command components in a WASI host.

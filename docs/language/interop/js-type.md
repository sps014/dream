---
title: The js type
summary: "Call and update live JavaScript values."
icon: material/language-javascript
status: stable
targets: [web, node]
related: [language/interop/index.md, language/interop/callbacks.md]
---

# The `js` type

Import `system.js`. A `js` value is a live JavaScript value. Member access, calls, and indexing type-check without a declared shape, and the host reports a missing member at runtime. The result stays `js` until you bind it to a Dream type.

```dream
import system;
import system.js;

fun main(): void {
    let element = js.global.document.getElementById("app");
    element.textContent = "hello";
    let count: int = element.childNodes.length;
    System.println(count);
}
```

`js` requires `--web` or `--node`.

| Value | What it is |
| --- | --- |
| `js.global` | `globalThis` |
| `js.global[name]` | `globalThis[name]` |
| `js.object()` | `{}` |
| `js.array()` | `[]` |
| `js.null` / `js.undefined` | those JavaScript values |

```dream
let options = js.object();
options.method = "POST";
js.global.fetch("/api", options);
```

## Conversions

Assigning `js` to a typed place converts it. `int`, `long`, `double`, `bool`, `string`, `js`, arrays of those, and Dream functions can be passed to JavaScript. A struct or class is copied into a plain object. `List` and unions are not.

| Method | Result |
| --- | --- |
| `to_int()` / `to_double()` / `to_bool()` / `to_str()` | that Dream type |
| `is_null()` | `true` for `null` or `undefined` |

```dream
class Point {
    public x: int;
    public y: int;

    public constructor(x: int, y: int) {
        this.x = x;
        this.y = y;
    }
}

let point: Point = js.global.origin();
js.global.render(Point(3, 4));
```

Reading a class or value struct from `js` fills the declared fields. It does not call the constructor.

## Awaiting JS Promises

Awaiting a promise returns `Result<js, JsError>`. Fulfillment, including `null`, is `Ok`. Rejection is `Err`. `error.reason` is the rejection value, and `error.message` is its text.

```dream
import system;
import system.js;

async fun main(): void {
    switch (js.global.fetch("https://example.com/api").await) {
        Ok(response) => System.println(response.ok),
        Err(error) => System.println(error.message),
    }
}
```

An HTTP error status is still `Ok`. Check `response.ok`.

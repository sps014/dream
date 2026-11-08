---
title: The js type
summary: "Read, call, and change live JavaScript values with ordinary syntax."
icon: material/language-javascript
status: stable
targets: [web, node]
related: [language/interop/index.md, language/interop/callbacks.md]
---

# The `js` type

`js` is a handle to a live JavaScript value — a DOM node, a `fetch` `Response`, a `RegExp`, a
plain object, or a function. You read, call, and mutate it with **ordinary Dream syntax**.

!!! glance "At a glance"

    ```dream
    let el = js.global.document.getElementById("app");
    el.textContent = "hello";
    ```

    - `js` is a handle to a live JavaScript value.
    - Read, call, and change it with ordinary Dream syntax; it needs a browser or Node host.

```dream
fun main(): void {
    let doc = js.global.document;
    let el = doc.getElementById("app");
    el.textContent = "hello";
    el.classList.add("a", "b", "c");

    let n: int = el.childNodes.length;
    System.println("children: " + n);
}
```

## How `js` values work

`js` is a static type, and Dream does not look up its members: any `.name`, call, or `[key]` type-checks, and the host decides whether it exists. The result is `js`, so `el.classList.add(...)` works.

You leave the dynamic world at a **typed boundary** — assigning to a typed variable, passing a typed
argument, or returning a typed value — where Dream converts automatically:

```dream
let count: int = config.count; // js -> int here
```

A `js` value is not a Dream object. Dream frees the handle when your program no longer uses it, including when the scope ends, you reassign it, or a container goes away — see [Memory Management](../memory/index.md).

## Getting a `js` value

| Entry point | Gives you |
| --- | --- |
| `js.global` | `globalThis` — e.g. `js.global.document`, `js.global.fetch(...)` |
| `js.global(name)` | `globalThis[name]`, for a runtime-only name |
| `js.object()` | a fresh empty `{}` |
| `js.array()` | a fresh empty `[]` |
| `js.null` / `js.undefined` | the JS `null` / `undefined` values as `js` handles |
| `js.func(handler)` / `js.func0(handler)` | wrap a Dream function as a JS callable — see [Callbacks](callbacks.md) |

```dream
let opts = js.object();
opts.method = "POST";
opts.keepalive = true;
js.global.fetch("/api", opts);
```

## Reading, writing, and calling

```dream
let el = js.global.document.getElementById("app");

let cls: js = el.className;
el.className = "highlighted";
el.classList.add("a", "b", "c");
let first = el.children[0];
el.children[0] = replacement;
```

## Passing values to JS

| Dream value | Crosses as |
| --- | --- |
| `int` / `long` / `double` / `bool` / `string` | itself |
| another `js` | passed through |
| an array of the above (`int[]`, `string[]`, `js[]`, …) | a JS array |
| a Dream function (`fun(...)`) | a JS callable — see [Callbacks](callbacks.md) |
| a `struct` / `class` | a **deep copy** into a plain JS object |

A `union` or `List<T>` is not marshalable directly — convert to an array or struct field first.

```dream
// js.global.send(my_list);   // error: List is not passed to js
// js.global.send(my_union);  // error: union is not passed to js
```

### Structs and classes

Passing a `struct` or `class` to JS deep-copies it into a plain object (nested fields and arrays
included). Fields that cannot cross (maps, interfaces, function values) are left off:

```dream
class Point {
    public x: int;
    public y: int;

    public constructor(x: int, y: int) {
        this.x = x;
        this.y = y;
    }
}

js.global.render(Point(3, 4)); // -> render({ x: 3, y: 4 })
```

Assigning a `js` object to a class or value struct reads each declared field and does not call the constructor. A class is created on the heap, and a value struct is filled in place:

```dream
class Point {
    public x: int;
    public y: int;

    public constructor(x: int, y: int) {
        this.x = x;
        this.y = y;
    }
}

let p: Point = js.global.originPoint();
```

```dream
struct Vec2 {
    public x: int;
    public y: int;
}

let v: Vec2 = js.global.originVec();
```

## Getting values back out

A JS result stays `js` until a typed binding, argument, or return converts it. You can also convert it yourself:

| Method | Converts to |
| --- | --- |
| `to_int()` / `to_double()` / `to_bool()` / `to_str()` | the matching Dream primitive |
| `is_null()` | `true` if the value is `null` or `undefined` |

## Awaiting JS Promises

Await a JS Promise directly. It resolves to `Option<js>`: `Some(value)` on success, and `None` when the Promise rejects or resolves to `null` or `undefined`.

A rejection reason is printed on the console. A typed `extern async fun` stops the program on rejection instead — see [Awaiting JavaScript promises](../concurrency/async.md#when-the-promise-rejects).

```dream
async fun load(): void {
    let resp = js.global.fetch("/api").await;
    switch (resp) {
        Some(r) => {
            let ok: bool = r.ok;
            System.println("ok = " + ok);
        }
        None => System.println("request failed"),
    }
}
```

`await` may appear anywhere in an `async` function, including loops and branches.

## Where it runs

`js` needs a JavaScript host (browser or Node). Referencing `js` APIs when targeting native is a compile error.

## Try it

- [`sample/interop/js.dream`](https://github.com/sps014/dream/blob/main/sample/interop/js.dream)
- [`sample/interop/slots.dream`](https://github.com/sps014/dream/blob/main/sample/interop/slots.dream)
- [`sample/interop/structs.dream`](https://github.com/sps014/dream/blob/main/sample/interop/structs.dream)
- [`sample/interop/value_structs.dream`](https://github.com/sps014/dream/blob/main/sample/interop/value_structs.dream)
- [`sample/interop/async_js.dream`](https://github.com/sps014/dream/blob/main/sample/interop/async_js.dream)

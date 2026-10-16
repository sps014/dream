---
title: Callbacks
summary: Pass functions across the Dream and JavaScript boundary.
icon: material/phone-return-outline
status: stable
targets: [web, node]
---

# Callbacks

Functions cross the Dream/JavaScript boundary in both directions: hand a Dream function to JS, or call a JS function from Dream. The runtime wraps Dream function values as ordinary callable JS functions.

!!! glance "At a glance"

    - Hand a Dream function to JavaScript, or call a JavaScript function from Dream.
    - The runtime wraps functions automatically in both directions.

**Only non-capturing functions may cross into JS.** A capturing lambda would lose its captured state at the boundary. Dream rejects capturing callbacks — use a top-level `fun`, a captureless lambda, or module-level state instead (see the [music player sample](https://github.com/sps014/dream/blob/main/sample/music_player/music_player.dream)).

```dream
// el.addEventListener("click", (ev) => { System.println(name); }); // error: capturing callback rejected
```

## Callback forms

| Form | Direction | Result |
|------|-----------|--------|
| Named `fun` or captureless lambda passed to a function-typed `extern` parameter | Dream to JS | JS receives an ordinary callable function |
| Named `fun` passed to a dynamic `js` call | Dream to JS | Wrapped automatically, with the same wrapper each time for the same function |
| `js.func(handler)` | Dream to JS | An explicit `js` callable for a `fun(js): void` |
| `js.func0(handler)` | Dream to JS | An explicit `js` callable for a `fun(): void` |
| Capturing lambda | Dream to JS | Compile error |
| JavaScript function received as a `js` value | JS to Dream | Called with ordinary call syntax |

Handlers with two or more `js` parameters, such as `fun(js, js): void`, are also wrapped automatically when passed to a dynamic `js` call.

## Dream to JS

Declare an `extern` parameter with a function type, then pass a Dream function where it is expected:

```dream
fun on_tick(n: int): void {
    System.println("tick " + n);
}

extern fun run_callback(cb: fun(int): void, times: int): void;

fun main(): void {
    run_callback(on_tick, 3); // on_tick crosses as a JS-callable wrapper
}
```

The host receives `cb` already wrapped and calls it like any JS function:

```js
await run("callbacks.wasm", {
  imports: {
    run_callback: (cb, times) => {
      for (let i = 0; i < times; i++) cb(i);
    },
  },
});
```

## Registering DOM handlers

Passing a Dream function directly into a dynamic [`js`](js-type.md) call wraps it automatically:

```dream
fun on_click(ev: js): void {
    System.println("clicked: " + ev.type.to_str());
}

fun main(): void {
    let el = js.global.document.getElementById("app");
    el.addEventListener("click", on_click);
}
```

The wrapper has **stable identity per function**, so a handler registered with `addEventListener` can later be removed by passing the *same* Dream function to `removeEventListener`. If you need an explicit `js` callable up front (to store, compare, or pass around first), build one with `js.func(handler)` / `js.func0(handler)` (still only with a captureless `fun(...)`).

## JS to Dream

A JavaScript function handed to Dream is just a [`js`](js-type.md) value — call it with ordinary Dream syntax, and its arguments auto-convert on the way in:

```dream
fun main(): void {
    js.global.logger("hello from Dream");
}
```

```js
globalThis.logger = (msg) => console.log("[logger]", msg);
await run("callbacks.wasm");
```

## Marshaling

Callback arguments and results follow the same conversion rules as ordinary externs and dynamic `js` calls (see [Value marshaling](index.md#value-marshaling) and [Passing values to JS](js-type.md#passing-values-to-js)): primitives and `string` convert automatically, and JS values travel as `js` handles.

## Try it

[`sample/interop/callbacks.dream`](https://github.com/sps014/dream/blob/main/sample/interop/callbacks.dream) runs both directions end to end, with its Node runner [`callbacks.mjs`](https://github.com/sps014/dream/blob/main/sample/interop/callbacks.mjs).

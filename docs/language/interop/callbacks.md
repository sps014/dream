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

Only a function that captures nothing may cross into JavaScript. A capturing lambda is rejected, so use a top-level `fun`, a captureless lambda, or module-level state (see the [music player sample](https://github.com/sps014/dream/blob/main/sample/music_player/music_player.dream)).

```dream
// el.addEventListener("click", (ev) => { System.println(name); }); // error: capturing callback rejected
```

## Callback forms

| Form | Direction | Result |
|------|-----------|--------|
| Named `fun` passed to a dynamic `js` call | Dream to JS | Wrapped automatically, with the same wrapper each time for the same function |
| `js.func(handler)` | Dream to JS | An explicit `js` callable for a `fun(js): void` |
| `js.func0(handler)` | Dream to JS | An explicit `js` callable for a `fun(): void` |
| Capturing lambda | Dream to JS | Compile error |
| JavaScript function received as a `js` value | JS to Dream | Called with ordinary call syntax |

Handlers with two or more `js` parameters, such as `fun(js, js): void`, are also wrapped automatically when passed to a dynamic `js` call.

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

The wrapper keeps a stable identity for each function, so `removeEventListener` can take the same Dream function you passed to `addEventListener`. To hold a `js` value before you pass it, call `js.func(handler)` or `js.func0(handler)` with a captureless function.

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

Callback arguments and results follow the same conversion rules as dynamic `js` calls (see [Value marshaling](js-type.md#passing-values-to-js) and [Passing values to JS](js-type.md#passing-values-to-js)): primitives and `string` convert automatically, and JS values travel as `js` handles.

## Try it

[`sample/interop/callbacks.dream`](https://github.com/sps014/dream/blob/main/sample/interop/callbacks.dream) runs both directions end to end, with its Node runner [`callbacks.mjs`](https://github.com/sps014/dream/blob/main/sample/interop/callbacks.mjs).

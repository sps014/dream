---
title: Callbacks
summary: Pass a Dream function to JavaScript.
icon: material/phone-return-outline
status: stable
targets: [web, node]
---

# Callbacks

Pass a named function or a captureless lambda. Dream wraps it as one JavaScript function and reuses that function when you pass it again. A lambda that captures a local is a compile error.

```dream
import system;
import system.js;

fun on_click(event: js): void {
    System.println(event.type.to_str());
}

fun main(): void {
    let element = js.global.document.getElementById("app");
    if !element.is_null() {
        element.addEventListener("click", on_click);
        element.removeEventListener("click", on_click);
    }
}
```

A dynamic call accepts the Dream function's own signature. A `jsimport` declaration uses the library's callback type. Parameters and results may be primitives, `js`, or one imported JavaScript object. When overloads name different objects for the same slot, that slot is `js`.

A function can also be stored as `js` and called:

```dream
import system.js;

fun twice(value: double): double {
    return value * 2;
}

fun main(): void {
    let callback: js = twice;
    js.global.console.log(callback(7));
}
```

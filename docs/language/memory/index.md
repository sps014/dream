---
title: Memory
summary: "How Dream frees objects, and the cases that need extra care."
icon: material/memory
---

# Memory

Dream frees an object when nothing uses it. You do not free it yourself.

A request that is too large stops the program. Array and string lengths use `int`. Use `isize` or `usize` when a size must match the platform.

## Guides in this section

<!-- pages -->

## What lives on the heap

- Strings
- Arrays (`T[]`)
- Class instances
- Standard library collections (`List`, `Map`, `Set`)

Primitives (`int`, `float`, `bool`, ...) and structs are stored inline.

A `js` value follows the same rules. When the last Dream name drops it, the JS value can be collected. See [The `js` type](../interop/js-type.md).

## What stays safe

- A value stays valid while you still use it.
- Each object is freed once.
- An out-of-bounds index stops the program.
- A store that would form a reference cycle is a compile error. Use `weak` or `unowned`, or `@allow_cycle` when your code breaks the cycle.
- A constructor runs when the object is created.
- A function local hides a top-level name with the same spelling.

`RawPtr<T>` and `Arena` are the manual option. They require `@unsafe`.

## When an object is freed

- A name leaving scope drops the value.
- Assigning a new value drops the old one. That includes a module-level `let`.
- At the last drop, `del` runs if the type has one, then the object is freed.
- Passing and assigning follows [ownership](ownership.md). An unmarked parameter takes the value, `borrow` only looks, and a last use hands the value over.

```dream
fun make_list(): int[] {
    let arr = [1, 2, 3];
    return arr;
}

fun main() {
    let result = make_list();
    System.println(result[0]);
} // result leaves scope and is freed
```

- Browser and Node free a value as soon as the last use ends.
- `dream run` may wait until the end of the block.

### Share a value with a task

Use a `shared` type when several tasks must use one value. Use `lock` before changing its fields. See [Tasks](../concurrency/tasks.md).

## Limits

| Limit | What you do |
|---|---|
| A field typed as `object` | A store that might form a cycle is a compile error. An `@allow_cycle` class leaks until your code breaks the cycle |
| A Dream object and a JS object pointing at each other | Pass an id instead |
| Changes from several tasks | Use `Lock` on a `shared class`. Dream does not reject races for you |

## On a hot path

- Build text with `StringBuilder`, not repeated `+`.
- `char_at` and `substring` count UTF-16 code units. `substring` shares the parent string. `clone` copies it.
- `List`, `Map`, and `Set` `clear()` keeps capacity.
- `Arena.reset()` drops the allocations and keeps the storage. `free()` releases the storage. See [`sizeof`](../basics/type-queries.md#sizeof-nameof-and-typeof).
- Mark a parameter `borrow` when it only reads. See [Ownership](ownership.md).

## Call stack

Deep recursion and large structs need a bigger stack. For `dream run`, set `DREAM_STACK_SIZE` (`32M`, `32MiB`, or a byte count). The default is 16 MiB. Values below 64 KiB are rejected.

The same variable sets the WebAssembly stack for `dream --wasm`.

```bash
DREAM_STACK_SIZE=32M dream run path/to/file.dream
```

`dream run` and `dream --wasm` use this value. Browser and Node use the engine's own stack.

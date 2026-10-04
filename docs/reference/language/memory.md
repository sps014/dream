# Memory Management

Dream cleans up objects automatically when they are no longer needed. Most programs can rely on this without managing memory themselves. This guide explains shared objects, cleanup timing, and the advanced cases that need extra care.

Available memory and the selected environment limit allocation size. A request that is too large, or an allocation that fails, stops the program with a clear error. Array and string lengths still use `int`. Use `isize` and `usize` when a size must match the selected platform.

## Explore this topic

- [Avoid reference cycles](memory-cycles.md)
- [Work with raw buffers](memory-buffers.md)
- [Choose when cleanup runs](memory-cleanup.md)

## What lives on the heap

- Strings
- Arrays (`T[]`)
- Class instances
- Standard library collections (`List`, `Map`, `Set`)

Primitives (`int`, `float`, `bool`, ...) and value `struct`s are stored inline — no heap allocation.

`js` handles are not Dream heap objects, but they follow the same ownership rules: when the last Dream owner drops, the JS value can be collected. See [The `js` type](js-type.md).

## Safety guarantees

Dream's type system and compiler enforce the following in safe code — no `unsafe` block, no annotation, no discipline required:

| Guarantee | How |
|---|---|
| No use-after-free | Values remain valid while they are still in use |
| No double-free | Automatic cleanup frees each object once |
| No out-of-bounds read | Bounds-checked indexing panics with a clear message |
| Cycle leaks rejected | Compile-time reference-cycle detection covers direct, indirect, tuple, value-struct, interface-typed, and closure-capture shapes |
| Constructor side effects preserved | Constructor bodies always run at allocation sites |
| Global/local scope separation | Top-level variables are file-scoped; function locals shadow them cleanly |

When these guarantees are not enough, the `@unsafe` tier (`Pointer<T>`, `Buffer.realloc`, `Buffer.free`) provides manual control with explicit opt-in.

## How it works

Every heap object tracks how many names still point at it.

- When a variable goes out of scope, that count goes down.
- Reassigning a variable drops the value it held before. Module-level `let` names follow the same automatic reference tracking rules as a field store (a still-live local copied into a global is retained; the previous occupant is released).
- When the count reaches zero, the object is freed (its `del` destructor runs first, if it has one).
- Passing and assigning heap values uses [ownership](ownership.md): unmarked parameters take ownership, `borrow` shares, and a last use **moves** instead of copying.

```dream
fun make_list(): int[] {
    let arr = [1, 2, 3];   // allocated, count = 1
    return arr;            // handed to the caller
}

fun main() {
    let result = make_list();
    System.println(result[0]);
} // result leaves scope -> count 0 -> freed
```

When you compile for the browser or Node, memory is freed as soon as the last use ends.
`dream run` may wait until the end of the block.

### Share values with a task

Use a `shared` type when a value must be accessible to several tasks. Sharing does not make changes to its fields safe by itself: use `lock` to coordinate those changes. Read [Tasks](tasks.md) for examples and the sharing restrictions.

## Known boundaries

The following are documented limitations, not silent unsoundness — each degrades to a detectable pattern or an explicit opt-out rather than memory corruption:

| Boundary | Status |
|---|---|
| Cycles routed through `object`-typed loose references | Deferred: requires runtime type introspection to trace |
| JS↔Dream cross-collector cycles | Interop boundary is weak-by-convention; use id-based protocols |
| Data races across threads | Use `Lock` on a `shared class`. Dream does not yet reject races for you |

## Performance notes

- Prefer `StringBuilder` (and `append` / `append_utf8_slice`) over repeated `string` `+` when building text in a loop.
- Use `byte_size` / `byte_at` for the raw UTF-16 bytes; `char_at` / `substring` index UTF-16 code units. `substring` is a cheap slice of the parent; `string.clone` copies.
- `List` / `Map` / `Set` `clear()` keeps capacity. Prefer `clear` + refill over allocating a new collection each batch.
- `ScratchArena<T : unmanaged>` is for short-lived scratch (`reset()` rewinds without returning memory to the OS) — parse/match/fill, not long-lived graphs. [`sizeof`](type-queries.md#sizeof-nameof-and-typeof) gives the byte size of an unmanaged element type.
- Unmarked parameters sink into the callee (see [Ownership](ownership.md)); mark readers `borrow`.
- Prefer `struct` / scalars / `Span` / dense `int[]` on hot paths.

## Call stack (`dream run`)

Deep recursion and large `struct` frames need enough call stack. For `dream run`, set **`DREAM_STACK_SIZE`** (e.g. `32M`, `32MiB`, or a byte count). The default is 16 MiB.

Values below 64 KiB are rejected. The same variable also sizes the wasm32 guest stack (`dream --wasm` links with `-z stack-size`).

```bash
DREAM_STACK_SIZE=32M dream run path/to/file.dream
```

This only affects native `dream run`.
Browser and Node use the engine's own stack limits.

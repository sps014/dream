# Memory Management

Dream manages heap memory with **automatic reference counting (ARC)**. You never call `free` — memory is reclaimed the moment the last reference to an object drops.

## What lives on the heap

- Strings
- Arrays (`T[]`)
- Class instances
- Standard library collections (`List`, `Map`, `Set`)

Primitives (`int`, `float`, `bool`, ...) and value `struct`s are stored inline — no heap allocation.

`js` handles are not Dream heap objects, but they follow the same ownership rules: when the last Dream owner drops, the JS value can be collected. See [The `js` type](js-type.md).

## How it works

Every heap object tracks how many names still point at it.

- When a variable goes out of scope, that count goes down.
- Reassigning a variable drops the value it held before. Module-level `let` names follow the same retain/release rules as a field store (a still-live local copied into a global is retained; the previous occupant is released).
- When the count reaches zero, the object is freed immediately (its `del` destructor runs first, if it has one).
- Passing and assigning heap values uses [ownership](ownership.md): unmarked parameters sink, `borrow` shares, and a last use **moves** instead of copying.

```dream
fun make_list(): int[] {
    let arr = [1, 2, 3];   // allocated, count = 1
    return arr;            // handed to the caller
}

fun main() {
    let result = make_list();
    println(result[0]);
} // result leaves scope -> count 0 -> freed instantly
```

## Advanced: reference cycles

ARC cannot collect a **cycle**. If `A` references `B` and `B` references `A`, neither count ever reaches zero — a leak:

```dream
class Node {
    public next: Option<Node>;
}

let a = Node(...);
let b = Node(...);
a.next = Option.Some(b);
b.next = Option.Some(a);   // cycle created — `a` and `b` now leak
```

### Dream catches this for you

Dream looks at every `class`'s strong (non-`weak`/`unowned`) fields and errors if those types can form a cycle — including a class holding a field of its own type:

```
error: reference cycle detected: 'Node.next' form a strong-reference cycle, so none of their
objects can ever be freed; mark one field 'weak' or 'unowned' to break it, or annotate every
class in the cycle with '@allow_cycle' if the cycle is intentional
```

This is a **type** check, not a value check: it flags "these classes *could* form a cycle," not "this program creates one." It follows strong fields through `Option<T>`, `T[]`, `List<T>`, `Map<K, V>`, and `Set<T>`. It cannot see cycles assembled dynamically through `object` or callbacks; those still require care.

### Breaking a cycle: `weak` and `unowned`

Mark one side of the cycle `weak` or `unowned` so it doesn't keep the other object alive:

```dream
class Node {
    public next: Option<Node>;
    weak parent: Option<Node>;    // does not keep the parent alive
}

class Cache {
    unowned owner: Manager;       // does not keep `owner` alive
}
```

- **`weak T`** — the field must be `Option<T>` for a class `T`. Read it like any other `Option`: `switch`, `.unwrap_or(...)`, `.is_some()`.
- **`unowned T`** — the field must itself be a class type `T` (not wrapped in `Option`). Use it only when you already know the other object outlives this one (e.g. "the parent always outlives the child").

A field marked either way is not part of the cycle check.

#### Runtime behavior

Neither modifier keeps the other object alive:

- **`weak`** fields become `Option.None` the instant the last strong reference is gone — you never observe a dangling pointer:

    ```dream
    class Node {
        public value: int;
        public weak parent: Option<Node>;
    }

    fun demo(child: Node) {
        let p = Node(...);
        child.parent = Option.Some(p);
        // ... p's only strong owner is this local ...
    } // `p` is released here -> `child.parent` becomes `Option.None`
    ```

- **`unowned`** is a promise ("this will always outlive me"). Reading one after the object is gone **panics**:

    ```
    panic: access to deallocated 'unowned' reference (at cache.dream:12, in main)
    ```

    Use `unowned` only when you can truly guarantee that; reach for `weak` (and a `switch` / `is_some()` check) whenever the lifetime is less certain.

## UI trees and DOM nodes

A render tree is two graphs that Dream does **not** treat as the same:

1. **Dream classes** — `parent` + `children: List<Node>` is a strong cycle unless `parent` is `weak` / `unowned`. A `List<Node>` field on `Node` is also a cycle through the collection; mark the class `@allow_cycle` if you keep strong children. Dropping the root then reclaims the tree.
2. **`js` DOM nodes** — `createElement` / `appendChild` keep the real JS object alive until the last Dream `js` handle is gone. `innerHTML = ""` or `removeChild` only drops the **browser** ref. A `js` temp that is never read after `appendChild` is released at that last use (not at `}`). If you keep a `List<js>` of every created node across frames, clear it (or drop the list) or the handles stay pinned even after the DOM is empty.

```dream
@allow_cycle
class Node {
    public children: List<Node>;
    public weak parent: Option<Node>;
}

fun rebuild() {
    let root = Node();
    // last *read* of `root` — the tree can be freed here
    System.println(root.id);
    do_unrelated_work();
}
```

Do not keep a second `List<js>` of every `createElement` result unless you `clear` it when you rebuild.

### `@allow_cycle`: the escape hatch

For the rare case where a cycle is intentional and manually managed, annotate **every** class in the cycle:

```dream
@allow_cycle
class Node {
    public next: Node;
    public prev: Node;   // you take responsibility for breaking this cycle
}
```

`@allow_cycle` only covers a cycle entirely inside the classes that carry it — annotating just one class in a multi-class cycle does not silence the rest.

## `@unsafe`: manual memory management

A handful of low-level primitives step outside ARC: `Buffer.realloc` / `Buffer.free` and [`Pointer<T>`](arrays.md#pointert-manual-allocation-unsafe) manage a block's lifetime yourself. Every function or method that touches one of these must be marked `@unsafe`:

```dream
@unsafe
fun grow(p: Pointer<int>): Pointer<int> {
    p.realloc(p.length * 2);   // fine: this function is itself @unsafe
    return p;
}

fun caller(): void {
    let p = Pointer<int>.alloc(4);
    grow(p);   // error: call to '@unsafe' function 'grow' is only allowed from
               // another '@unsafe' function or method
}
```

Calling an `@unsafe` function from ordinary code is a compile-time error. Marking your own function `@unsafe` means *its* callers must be `@unsafe` too — the attribute has to be threaded all the way up to wherever the unsafe operation is justified.

`@unsafe` does **not** insert runtime checks, and it does not verify the contract of the operation you're calling (e.g. that a freed `Pointer<T>` is never read again). It is a documented promise from the author, not a proof.

## `defer`: wait until after the important work to run destructors

Normally, when nothing points at an object anymore, Dream runs its `del` (if any) and frees it **right then**. That is what you want almost everywhere.

Sometimes that “right then” is a bad moment: you drop last year’s UI tree or a particle buffer, and the destructor storm runs **before** you finish drawing or simulating this frame. `defer { … }` keeps the objects logically gone (nothing can use them), but **runs the actual cleanup at `}`** — after the work you care about.

```dream
defer {
    old_root = new_tree();   // old tree is no longer needed
    paint(new_tree);         // do this before a huge cleanup hitch
} // `del` / free of the old tree runs here
```

**Use it** when there is a deadline in the middle of a tick (paint, simulate, submit a frame) and a large graph dies in the same tick.

**Skip it** when you are just allocating and dropping in a loop with nothing urgent in between. Cleanup is not cheaper with `defer` — it is only **later** (and can use a bit more memory until `}`). Needless `defer` is extra bookkeeping.

Braces are required. `await` is not allowed inside `defer`. GPU shaders do not support it.

```dream
class Tracked {
    public id: int;
    del() { System.println(this.id); }
}

fun main() {
    defer {
        let x = Tracked(1);
        System.println(x.id);  // last use of `x` — `del` waits
        System.println(999);
    } // now prints 1
}
```

### Optional: how much to clean at `}`

- `defer { … }` — clean a batch at `}` (256 objects), then **finish the rest** if this is the outermost `defer`, so work does not sit until the program exits.
- `defer(q) { … }` — `q` is a `uint` (plain `256` is fine). That many objects are cleaned at this `}`. `defer(0)` means “don’t clean on this `}`” — useful around a game loop so inner `defer(256)` slices can spread cleanup across frames.
- Nested `defer` share one cleanup list. While you are still inside some `defer`, leftover work can wait for the next one, but the list is capped (16 384 objects) so memory cannot grow without bound.
- `dream run` (native) is where this queue is real. Compiling to WebAssembly still frees immediately today.
- On native, a last-ref **string** that the compiler releases is queued like other last-refs (the free waits for `}`). Helpers such as in-place concat still free string temps immediately.

A timing sample (UI tree swap + particles): `dream --release run sample/defer_destroy_bench.dream`.

## Performance notes

- Prefer `StringBuilder` (and `append` / `append_utf8_slice`) over repeated `string` `+` when building text in a loop.
- Use `byte_size` / `byte_at` for the raw UTF-16 bytes; `char_at` / `substring` index UTF-16 code units. `substring` is a cheap slice of the parent; `string.clone` copies.
- `List` / `Map` / `Set` `clear()` keeps capacity. Prefer `clear` + refill over allocating a new collection each batch.
- `ScratchArena<T : unmanaged>` is for short-lived scratch (`reset()` rewinds without returning memory to the OS) — parse/match/fill, not long-lived graphs. [`sizeof`](operators.md#sizeof-and-nameof) gives the byte size of an unmanaged element type.
- Unmarked parameters sink into the callee (see [Ownership](ownership.md)); mark readers `borrow`.
- Prefer `struct` / scalars / `Span` / dense `int[]` on hot paths.

## Call stack (`dream run`)

Deep recursion and large `struct` frames need enough call stack. For `dream run`, set **`DREAM_STACK_SIZE`** (e.g. `32M`, `32MiB`, or a byte count). The default is 16 MiB. Values below 64 KiB are rejected.

```bash
DREAM_STACK_SIZE=32M dream run path/to/file.dream
```

This only affects native `dream run`. Browser and Node use the engine's own stack limits.

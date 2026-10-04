# Avoid reference cycles

A reference cycle keeps objects pointing to one another. Use this guide when relationships in your data could form a loop.

[Back to overview](memory.md)

## Advanced: reference cycles

ARC cannot collect a **cycle**.
If `A` references `B` and `B` references `A`, neither count ever reaches zero — a leak:

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

This is a **type** check, not a value check: it flags "these classes *could* form a cycle," not "this program creates one."
It follows strong fields through `Option<T>`, `T[]`, `List<T>`, `Map<K, V>`, and `Set<T>`.
It cannot see cycles assembled dynamically through `object` or callbacks; those still require care.

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

Weak loads and target clearing use the same runtime lock. A successful load retains the
target before unlocking, so concurrent destruction cannot leave the reader with a dangling
reference. This lifetime protection does not synchronize application fields: shared mutation
still needs `Lock` or another explicit synchronization mechanism.

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
2. **`js` DOM nodes** — `createElement` / `appendChild` keep the real JS object alive until the last Dream `js` handle is gone. `innerHTML = ""` or `removeChild` only drops the **browser** ref.
   A `js` temp that is never read after `appendChild` is released at that last use (not at `}`). If you keep a `List<js>` of every created node across frames, clear it (or drop the list) or the handles stay pinned even after the DOM is empty.

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

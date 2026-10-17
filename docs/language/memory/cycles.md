---
title: Reference cycles
summary: "Break strong cycles with `weak` and `unowned` links."
icon: material/autorenew
status: stable
targets: [native, web, node]
related: [language/memory/ownership.md, language/memory/cleanup.md]
---

# Cycles and ARC

Dream uses deterministic reference counting in Debug and Release. ARC does not collect
strong reference cycles. Break cycles with `weak` or checked `unowned` back-links, or
explicitly clear an owning edge before releasing the last external owner.

!!! glance "At a glance"

    ```dream
    @allow_cycle
    class Node {
        public next: Option<Node>;
    }
    ```

    - Reference counting does not collect strong cycles.
    - Mark cycle-capable classes with `@allow_cycle` and break cycles with `weak`, `unowned`, or explicit teardown.

## Cycle-capable classes

A class whose strong field layout can form a cycle requires `@allow_cycle`. This check
follows concrete generic fields, arrays, containers, tuples and reference-bearing values.
Owning erased `object`, interface and closure fields are conservative: their runtime
referents may point back to the owner. Weak and unowned edges do not participate.

```dream
@allow_cycle
class Node {
    public next: Option<Node>;

    public constructor() {
        this.next = Option.None;
    }
}

fun main() {
    let node = Node();
    node.next = Option.Some(node);
    node.next = Option.None; // Explicit teardown releases the internal owner.
}
```

The annotation acknowledges possible leaks; it does not enable collection. Incorrect
all-strong cycles stay allocated, and Debug leak diagnostics report their outstanding
objects. Recursive reference types remain representable; infinitely sized inline value
types are rejected even when annotated.

## Destructors and observers

A finalizer runs once when the strong count reaches zero. Weak and unowned observers are
invalidated before user finalizers run. Owned fields are cleared afterward, then storage
is reclaimed. Retaining an object after zero is forbidden resurrection. There is no
special dying-peer access or finalizer order for leaking cycles.

Debug and Release share these ownership semantics. Explicit `defer` intentionally
postpones cleanup; deferred work drains before leak reporting.

## Weak and unowned references

| Field kind | Declared type | Keeps the target alive | Reading after the target is freed |
|------------|---------------|------------------------|-----------------------------------|
| Strong (default) | Any type | Yes | Not possible while the field holds it |
| `weak` | `Option<T>` for a class `T` | No | Gives `None` |
| `unowned` | A class type `T` | No | Traps |

`weak` and `unowned` can each be combined with `public`, but not with each other.

A `weak` field has type `Option<T>` for a class `T` and becomes `None` when the target dies.
An `unowned` field has class type and traps when read after invalidation. Reads acquire a
temporary strong reference that survives the consuming expression's cleanup boundary.
Neither field extends the target's lifetime while stored.

```dream
class Parent {
    public weak child: Option<Child>;
}

class Child {
    public unowned parent: Parent;
}
```

These fields have no strong cycle, so neither class needs `@allow_cycle`. A recursive
strong child field still makes a class cycle-capable, even when actual instances form a tree.

## Weak callback captures

A callback stored on an object can capture a weak local to avoid owning that object.
Read the binding with `switch` to handle a target that has already been destroyed:

```dream
class Counter {
    public callback: fun(): int;
    public value: int;

    public constructor() {
        this.value = 0;
        const weak observer = this;
        this.callback = () => {
            return switch (observer) {
                Some(counter) => counter.value,
                None => -1,
            };
        };
    }
}
```

`const weak` keeps the binding pointed at its original target. Use `let weak` when it
must be rebound. Both accept a class instance or `Option<T>` and read as `Option<T>`.
Clearing a mutable weak binding with `Option.None` is visible to closures that capture it.

## Concurrency

Published objects use atomic reference counts. A dedicated registry lock synchronizes
weak/unowned reads, registration and observed-target destruction. User finalizers run
outside it. Objects without weak observers need no registry lookup or lock. Application
code remains responsible for synchronization of concurrent field mutation.

Cleanup is deterministic for the same synchronized execution. Concurrent scheduling does
not gain a global destruction-order guarantee.

## Private regions

Release can infer a `UniqueRegion` for proven nonescaping, destructor-free allocations
and reclaim their storage in bulk. This is an internal optimization, not a public graph
owner or a guarantee that arbitrary strong cycles are reclaimed.

---
title: Reference cycles
summary: "Break strong cycles with `weak` and `unowned` links."
icon: material/autorenew
status: stable
targets: [native, web, node]
related: [language/memory/ownership.md, language/memory/cleanup.md]
---

# Cycles

A cycle keeps every object in it alive, so a store that would create one is a compile error. Use `weak` or `unowned` for a back-link, or mark the class `@allow_cycle` and break the cycle yourself.

!!! glance "At a glance"

    ```dream
    class Node {
        public next: Option<Node>;
    }

    fun link(a: Node, b: Node) {
        a.next = Option.Some(b); // ✓ a list, not a cycle
        b.next = Option.Some(a); // ✗ error: reference cycle: Node.next -> Node
    }
    ```

    - A store that links an object back to itself is an error.
    - Use `weak` or `unowned`, or `@allow_cycle` when your code breaks the cycle.

## Cycle checking

A linked list or a tree is fine. The error is a store that points an object back at itself. `weak` and `unowned` do not count as that kind of link. The check is the same in debug and release builds.

```dream
class Node {
    public next: Option<Node>;

    public constructor() {
        this.next = Option.None;
    }
}

fun main() {
    let first = Node();
    let second = Node();
    first.next = Option.Some(second);
    second.next = Option.Some(first);
}
```

The second store closes a cycle, so compilation fails with an error that names every link
in the cycle and the store that closes it:

```text
error: reference cycle: Node.next -> Node
note: objects in this cycle keep each other alive, so none of them is ever freed
note: the cycle is closed by the store at main.dream:13
help: make 'Node.next' weak or unowned so it does not keep its target alive
help: if the program breaks this cycle itself, add @allow_cycle to class 'Node'
```

Longer cycles list each step, for example
`Owner.handler -> Handler -> HandlerImpl.owner -> Owner`. When the compiler cannot prove
that a store stays acyclic, it reports the store; make one link non-owning or restructure
the code so the back reference is not stored.

## Program-managed cycles

Some designs keep a strong cycle on purpose and break it at a known point, such as a
session that owns its peers while each peer holds the session. Mark such a class with
`@allow_cycle`:

```dream
import system.collections;

@allow_cycle
class Session {
    public peers: List<Peer>;

    public constructor() {
        this.peers = List<Peer>();
    }

    public fun close(): void {
        this.peers.clear(); // Breaks every cycle through this session.
    }
}

class Peer {
    public session: Session;

    public constructor(session: Session) {
        this.session = session;
    }
}
```

The compiler then accepts cycles that pass through a `Session` object, but still reports a
cycle that bypasses every `@allow_cycle` object. The annotation does not enable
collection: a cycle the program never breaks stays allocated, and Debug leak diagnostics
report its objects. Infinitely sized inline value types are rejected even when annotated.

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

Neither field keeps its target alive, so linking a parent and child both ways never forms
a strong cycle.

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

## Several tasks

`weak` and `unowned` are safe to read from more than one task. You still coordinate changes to the object's fields yourself. Cleanup order is defined for one task. It is not a global order across tasks.

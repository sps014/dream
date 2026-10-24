---
title: Memory safety
summary: The rules that prevent invalid memory access in ordinary code.
icon: material/shield-check-outline
status: stable
targets: [native, web, node]
---

# Memory Safety Guide

Dream checks ordinary code to prevent invalid memory access and checks potentially cyclic class ownership. This guide explains the rules you will meet while writing safe code and the boundaries that still need care.

!!! glance "At a glance"

    ```dream
    public fun increment(): void {
        this.count = this.count + 1; // inferred unique access
    }
    ```

    - Ordinary code cannot read freed memory or index out of bounds.
    - Manual memory operations require an explicit `@unsafe` opt-in.

## Quick reference: what the compiler checks

| Check | Severity | Example rejected |
|---|---|---|
| Borrow contract violation | error | declared `borrow fun` that mutates |
| Interface value you call the method on-mode mismatch | error | implementor mode ≠ interface mode |
| Iterator/Span invalidation (same function) | error | `xs.push(2)` while cursor live |
| Cross-function stale view | error | view returned from method, then mutation |
| Container rewind without clearing slots | safe | old elements stay until overwrite or drop |

## The core memory model

ARC (automatic reference counting) reclaims objects deterministically.
Three rules hold unconditionally:

1. **Slot overwrite releases** — writing into any array/field releases the old value.
2. **Slot reads retain** — reading gives you a valid owned reference.
3. **Array drop releases all slots** — even slots a counter abandoned.

These mean raw buffers (`T[]` + manual counters) are always memory-safe.
Rewinding a counter is safe — no error.
Old elements stay until overwritten or the array is dropped.

## Receiver modes

Every method's implicit `this` has a mutation contract inferred from its body:

- **Borrow**: reads `this`, never mutates.
- **Unique**: may write fields or call other mutating methods.

```dream
class Counter {
    count: int;

    // Inferred borrow — reads only.
    public fun value(): int {
        return this.count;
    }

    // Inferred unique — writes this.count.
    public fun increment(): void {
        this.count = this.count + 1;
    }
}
```

Pin explicitly when needed:

```dream
borrow fun audit_only(): void {
    ...
} // compiler rejects mutations
unique fun must_mutate(): void {
    ...
} // always allowed
```

Interface implementors must match the interface's declared mode.

## Borrow checking: views and cursors

Iterators and `Span<T>` are *views* into a collection.
Mutating the collection while a view is live is a compile error:

```dream
let xs = List<int>();
let cur = xs.iterator();
xs.push(2); // ✗ error: mutates while 'cur' is live
cur.next(); // would read stale data
```

Fix: finish with the cursor first, then mutate:

```dream
cur.next();
xs.push(2); // ✓ legal — cursor no longer referenced
```

This works through field chains (`this.items.iterator()`), local aliases, and cross-function calls (methods returning views of their parameters propagate the borrow).

## Reference-cycle detection

ARC does not collect strong cycles, so the compiler rejects any store that can link an object back to itself through strong references. Recursive classes are allowed; only the stores that would close a cycle are errors. The check runs in Debug and Release and follows fields, containers, tuples, inline structs, interface values, closure captures and calls.

### Reported cycles

```dream
class A {
    public b: Option<B>;

    public constructor() {
        this.b = Option.None;
    }
}

class B {
    public a: Option<A>;

    public constructor() {
        this.a = Option.None;
    }
}

fun connect(a: A, b: B) {
    a.b = Option.Some(b);
    b.a = Option.Some(a); // ✗ reference cycle: B.a -> A.b -> B
}
```

The error names each link of the cycle and the store that closes it, and suggests which field to make non-owning.

### Breaking cycles

Mark one direction as non-owning:

```dream
class B {
    public weak a: Option<A>; // weak: does not keep its target alive
}
```

When a class keeps strong cycles on purpose and the program breaks them itself, mark the class `@allow_cycle`. Cycles through its objects are accepted; cycles that bypass them are still reported. See [reference cycles](cycles.md#program-managed-cycles).

## Closure capture safety

Closures own their captured references. Cyclic callback ownership can leak; clear callbacks explicitly or use non-owning handles when a callback should not extend its target lifetime.

## Weak locals and closure captures

Use `let weak` when a local or callback should observe a class instance without keeping it alive.
A read returns `Option<T>`: `Some` acquires a strong reference to a live target, and `None`
means the target has been destroyed. `const weak` prevents rebinding.

```dream
import system;

class Engine {
    public on_tick: fun(): void;
    public running: bool;

    public constructor() {
        this.running = false;
        this.on_tick = () => {};
    }

    public fun start(): void {
        let weak observer = this;
        this.on_tick = () => {
            switch (observer) {
                Some(engine) => engine.do_tick(),
                None => {},
            }
        };
    }

    public fun do_tick(): void {
        this.running = true;
    }
}
```

The initializer must be a class instance or an `Option` containing a class instance.
The callback captures the weak binding, so storing it on the engine does not keep the engine alive.
Use the value returned by a single read when checking and accessing the target.

Reassigning a mutable weak binding changes the observed target, including for closures sharing it:

```dream
let first = Engine();
let second = Engine();
let weak observer = first;
observer = second;
observer = Option<Engine>.None;
```

Assign `Option.None` to clear the binding. Weak locals cannot use tuple destructuring,
and `unowned` is restricted to fields. Numbers, strings and value structs cannot be weak targets.

## Container safety

### Rewinding counters

Pure rewinding (`count = 0`) without clearing slots is safe — dead elements stay retained until overwritten or until the container drops.
No compile error, no leak.

### Eager reclamation

Use `List.clear()` or remove entries from a list to release its stored references immediately.
For arrays of optional references, assigning `Option.None` releases the previous slot value.
Overwriting an array element also releases the previous value.

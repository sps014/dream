# Memory Safety Guide

Dream checks ordinary code to prevent invalid memory access and reclaims strong reference cycles deterministically. This guide explains the rules you will meet while writing safe code and the boundaries that still need care.

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
    public fun value(): int { return this.count; }

    // Inferred unique — writes this.count.
    public fun increment(): void { this.count = this.count + 1; }
}
```

Pin explicitly when needed:

```dream
borrow fun audit_only(): void { ... }     // compiler rejects mutations
unique fun must_mutate(): void { ... }    // always allowed
```

Interface implementors must match the interface's declared mode.

## Borrow checking: views and cursors

Iterators and `Span<T>` are *views* into a collection.
Mutating the collection while a view is live is a compile error:

```dream
let xs = List<int>();
let cur = xs.iterator();
xs.push(2);                    // ✗ error: mutates while 'cur' is live
cur.next();                    // would read stale data
```

Fix: finish with the cursor first, then mutate:

```dream
cur.next();
xs.push(2);                    // ✓ legal — cursor no longer referenced
```

This works through field chains (`this.items.iterator()`), local aliases, and cross-function calls (methods returning views of their parameters propagate the borrow).

## Reference-cycle detection

If class fields point at each other in a loop, that program is rejected.

### Direct and indirect field cycles

```dream
// Direct self-reference
class Node { next: Option<Node>; }                        // ✗ cycle

// Indirect via two classes
class A { b: B; }  class B { a: A; }                      // ✗ cycle

// Via tuples
class Data { r: (Data, int); }                            // ✗ cycle

// Via reference-holding value structs
struct Holder { d: Data; }
class C { h: Holder; }                                    // ✗ detected
```

### Interface-typed fields (conservative)

When a field references an interface, the graph includes edges to every implementing class.
This may report cycles that are only potential at runtime — use `weak` on one direction if the pattern is safe.

### Breaking cycles

Mark one direction as non-owning:

```dream
class Node {
    weak next: Option<Node>;       // weak: does not keep target alive
}
```

Strong cyclic ownership is also supported automatically.

## Closure capture safety

Closures own their captured references. Callback graphs can be cyclic; exact capture metadata
lets ARC reclaim them when external owners disappear. Use weak captures when a callback
should not extend its target lifetime.

## Weak handles

`Weak<T>` is a non-owning handle for breaking cycles that static analysis cannot prove away.
It is distinct from a field marked `weak`:

```dream
import system;

public class Engine {
    public on_tick: fun(): void;
    public running: bool;

    public fun start(): void {
        let we = Weak<Engine>(this);
        this.on_tick = () => {
            switch (we.get()) {
                Some(engine) => engine.do_tick(),
                None => {},
            }
        };
    }
    public fun do_tick(): void { /* ... */ }
}
```

- `Weak<T>(obj)` does not keep its target alive.
- When the target drops, `is_dead()` flips to true automatically.
- `get()` returns `Option<T>`, retaining a live target atomically. An `is_dead()`
  check followed by a separate load cannot protect against concurrent destruction.
- `T : class` requires a reference type; unmanaged scalar values cannot be weak targets.
- `release()` empties the handle and is safe to call repeatedly.
- Capture the handle in closures — not the raw object.

## Container safety

### Rewinding counters

Pure rewinding (`count = 0`) without clearing slots is safe — dead elements stay retained until overwritten or until the container drops.
No compile error, no leak.

### Eager reclamation

Use `Buffer.clear(arr)` / `Buffer.truncate(arr, n)` to release elements immediately:

```dream
Buffer.clear(this.entries);          // release all managed elements now
this.count = 0;
```

### Buffer.realloc shrinkage

Shrinking via `Buffer.realloc` automatically releases dropped tail slots — truncation never strands retained elements.

## Debugging retention

Debug builds (including `-g`) always print heap counters at exit:

```
[dream] leak check: live=0 total_allocations=6
```

Use `Debug.live_objects` deltas to assert balance in tests:

`Debug.live_objects` and `Debug.total_allocations` return `long` on native and wasm32.
Their 64-bit counters use relaxed atomic operations. Concurrent snapshots are diagnostic
observations rather than a synchronized view of every thread; check balances after workers
have completed. Pinned immortal singletons are excluded from the live count.

```dream
let before = Debug.live_objects;
churn();
let after = Debug.live_objects;
System.println((after - before).to_string());   // expect "0"
```

Release builds opt in via `DREAM_DEBUG_LEAKS=1`.

## Known boundaries

These are documented limitations, not silent unsoundness:

| Boundary | Status |
|---|---|
| Cycles through `object`-typed loose references | Deferred: requires runtime type introspection |
| JS↔Dream interop cycles | Interop boundary is weak-by-convention |
| Data races across threads | Use a lock. Dream does not yet reject races for you |

Every other pattern is either rejected at compile time or safely handled by ARC.

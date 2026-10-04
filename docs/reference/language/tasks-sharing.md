# Share state between tasks

Use this guide after creating a task. It explains which values can be shared and how to protect changes.

[Back to overview](tasks.md)

## Sharing state safely

A task body may be a **capturing lambda** — as long as everything it captures is either `shared` or **moved**:

- a blittable / unmanaged local,
- a `string`,
- a value struct of `shared` fields,
- a **`shared class`** instance (captured by reference, guarded by its lock word), or
- a **managed heap value moved by pointer hand-off** — an ordinary class instance, an array (`int[]`, `Job[]`), `List<T>`, … Tasks share the parent's memory, so the move is zero-copy: the object's ownership transfers and **the sender's binding cannot be used afterwards** (a use after the move is a compile error until it is reassigned).

What stays rejected: anything that would copy a non-shared *reference* across the boundary without transferring ownership — e.g. a value struct embedding an ordinary class field.

```dream
async fun main(): void {
    let nums = [1, 2, 3];
    let r = Task.spawn(() => nums.length);        // nums moves into the task
    // System.println(nums.length);               // error: use of 'nums' after move
    System.println(r.await);                      // 3
}
```

```dream
shared class Counter {
    public value: int;
    public constructor() { this.value = 0; }

    public fun increment(): void {
        lock (this) {
            this.value = this.value + 1;
        }
    }
}

async fun main(): void {
    let counter = Counter();

    let a = Task.spawn(() => { counter.increment(); return 0; });
    let b = Task.spawn(() => { counter.increment(); return 0; });

    a.await;
    b.await;

    System.println(counter.value);   // 2
}
```

`lock (obj) { ... }` is a reentrant mutual-exclusion block and requires a `shared class` (a lock word), not every `shared` type.
A `shared class`'s fields must be `shared` or managed heap references whose graph joins the object's shared region — see the [closed-graph field rule](classes-structs.md).

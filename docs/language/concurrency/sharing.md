---
title: Task sharing
summary: Share values between tasks and protect changes with locks.
icon: material/share-variant-outline
status: stable
targets: [native, web, node]
---

# Share state between tasks

Use this guide after creating a task. It explains which values can be shared and how to protect changes.

!!! glance "At a glance"

    ```dream
    let nums = [1, 2, 3];
    let r = Task.spawn(() => nums.length); // nums moves into the task
    ```

    - Values captured by a task move into it.
    - Use `shared` types and `lock` when several tasks must change the same data.

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
    let r = Task.spawn(() => nums.length); // nums moves into the task
    // System.println(nums.length);               // error: use of 'nums' after move
    System.println(r.await); // 3
}
```

```dream
shared class Counter {
    public value: int;

    public constructor() {
        this.value = 0;
    }

    public fun increment(): void {
        lock (this) {
            this.value = this.value + 1;
        }
    }
}

async fun main(): void {
    let counter = Counter();

    let a = Task.spawn(
        () => {
            counter.increment();
            return 0;
        }
    );
    let b = Task.spawn(
        () => {
            counter.increment();
            return 0;
        }
    );

    a.await;
    b.await;

    System.println(counter.value); // 2
}
```

`lock (obj) { ... }` is a reentrant mutual-exclusion block and requires a `shared class` (a lock word), not every `shared` type.
A `shared class`'s fields must be `shared` or managed heap references whose graph joins the object's shared region — see the [closed-graph field rule](../types/classes-structs.md).

## What a task can capture

| Captured value | Allowed | What happens |
|----------------|---------|--------------|
| Number, `bool`, `char`, or other unmanaged value | Yes | Copied into the task |
| `string` | Yes | Shared with the task |
| Struct whose fields are all `shared` | Yes | Copied into the task |
| `shared class` instance | Yes | Shared by reference; guard changes with `lock` |
| Ordinary class instance, array, `List<T>`, or other heap value | Yes | Moved into the task; the original variable cannot be used afterwards |
| Struct holding an ordinary class field | No | Compile error |

## Locks and semaphores

`Lock` and `Semaphore` are `shared` classes available without an import, for cases where a `lock` block does not fit, such as giving up after a timeout.

| Member | Returns | Effect |
|--------|---------|--------|
| `Lock()` | `Lock` | A reentrant lock |
| `Semaphore(initial)` | `Semaphore` | A counting semaphore with `initial` permits |
| `.acquire()` | — | Waits until the lock or a permit is available, then takes it |
| `.try_acquire()` | `bool` | Takes it only if it is available right now |
| `.try_acquire_for(timeout_ms)` | `bool` | Waits up to `timeout_ms` milliseconds; `0` or less is a single try |
| `.release()` | — | Releases the lock or returns one permit |

A thread may acquire a `Lock` it already holds, and must call `release` once per `acquire`. Releasing a `Lock` the current thread does not hold panics.

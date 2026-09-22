# Tasks

**Package:** `system.task` — `import system.task;` for `Task` / `TaskPool`. Console examples below also use `import system;`.

Dream's [`async`/`await`](async.md) is a *single-threaded* scheduler: work interleaves at `await` points but never runs at the same instant.
When you need more than one core — CPU-bound work or parallel pipelines — use a **`Task`**.

```dream
import system;
import system.task;

fun greet(name: string): string {
    return "hello, " + name + "!";
}

async fun main(): void {
    let name = "dream";
    System.println(Task.spawn(() => greet(name)).await);   // hello, dream!

    let n = 6;
    System.println((Task.spawn(() => n * n).await).to_string()); // 36
}
```

`Task.spawn(() => …).await` starts a body on its own OS thread (native) or Web Worker (browser) and waits for the result.
There is no spawn message and no `join()`.
Each task has its own **private memory** (ordinary `new`, strings, lists, and private globals).

`@shared class` instances, `Lock` / `Semaphore`, and `CancellationToken` live in **one shared place** both sides can see, so pointers and `lock` still work across tasks.

Captures and the body's return type must be **`shared`**: a blittable value, `string`, a value struct of `shared` fields, or an `@shared class`.
Ordinary classes, arrays, and `List` may **move** into the task (exclusive ownership; the sender cannot use the binding afterwards).
Capturing them by shared reference is a compile error.

!!! note "Browser status"
    The browser runtime (`runtime/dream.js`) shares memory across every spawned `Worker`, matching native — but the host page must be served with the [Cross-Origin Isolation](https://developer.mozilla.org/en-US/docs/Web/API/crossOriginIsolated) headers (`Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp`, or `credentialless`) or shared memory allocation fails silently in some browsers. `@shared` capture across tasks needs those headers.

## The model

```
┌────────────────────────────┐            ┌────────────────────────────┐
│      Owner instance        │            │       Task instance        │
│   (its own private memory) │            │   (its own private memory) │
│                            │            │                            │
│  Task.spawn(               │── env ────►│       body(): TOut         │
│        () => body).await   │  captures  │    (starts immediately)    │
│                            │            │                            │
│                            │◄─ result ──│                            │
│                            │            │                            │
└─────────────┬──────────────┘            └──────────────┬─────────────┘
              │                                          │
              └──────────────────┬───────────────────────┘
                                 ▼
                ┌────────────────────────────────────────┐
                │  Shared objects (one place both can    │
                │  see): @shared / lock / moved values   │
                └────────────────────────────────────────┘
```

- **The task body is a function value** — a top-level function or a lambda.
- **Captures must be `shared` or moved.** Overlap work by not awaiting yet: `let a = Task.spawn(...); let b = Task.spawn(...); a.await; b.await;`.

## API

```dream
public class Task {
    public static async fun spawn<TOut : shared>(body: fun(): TOut): TOut;
    public static async fun spawn_async<TOut : shared>(body: fun(): Future<TOut>): TOut;
    public static async fun map<T : shared, TOut : shared>(items: T[], body: fun(T): TOut): TOut[];
    public static async fun map_async<T : shared, TOut : shared>(items: T[], body: fun(T): Future<TOut>): TOut[];
}
```

`TOut` is inferred from the body.
Cancelling or dropping the spawn Future hard-aborts the task (`Promise.cancel`).
Prefer a captured `@shared` `CancellationToken` for cooperative cancel.

## Spawn

```dream
import system;
import system.task;

fun greet(name: string): string {
    return "hello, " + name + "!";
}

async fun main(): void {
    let name = "dream";
    System.println(Task.spawn(() => greet(name)).await);   // hello, dream!

    let n = 6;
    System.println((Task.spawn(() => n * n).await).to_string()); // 36
}
```

## Running work in parallel

Save the Futures before the first `await` so they compute concurrently:

```dream
fun work(input: string): string {
    let i = 0;
    while i < 5000000 { i = i + 1; }
    return input.to_upper();
}

async fun main(): void {
    let w1 = Task.spawn(() => work("alpha"));
    let w2 = Task.spawn(() => work("beta"));
    let w3 = Task.spawn(() => work("gamma"));

    System.println(w1.await);   // ALPHA
    System.println(w2.await);   // BETA
    System.println(w3.await);   // GAMMA
}
```

For the common "run N independent computations and collect the results" shape, `Task.map` (below) does this without manual bookkeeping.

## `Task.map` — parallel map

`Task.map` fans a body out over an array in parallel and collects replies **in input order** — one task per element.
The array stays on the owner; each element is a `shared` value copied into that task:

```dream
fun square(x: string): string {
    let n = int.parse(x).unwrap_or(0);
    return (n * n).to_string();
}

async fun main(): void {
    let items = ["1", "2", "3", "4", "5"];
    let results = Task.map(items, square).await;
    for (let r in results) {
        System.println(r);   // 1, 4, 9, 16, 25
    }
}
```

The `body` argument follows the same capture rules as `spawn`.

## Sharing state safely

A task body may be a **capturing lambda** — as long as everything it captures is either `shared` or **moved**:

- a blittable / unmanaged local,
- a `string`,
- a value struct of `shared` fields,
- an **`@shared class`** instance (captured by reference, guarded by its lock word), or
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
@shared
class Counter {
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

`lock (obj) { ... }` is a reentrant mutual-exclusion block and requires an `@shared class` (a lock word), not every `shared` type.
An `@shared class`'s fields must themselves be `shared` (the closed-graph rule).

## Cancellation

Two layers:

**Cooperative (preferred):** capture an `@shared` `CancellationToken` and poll it; the owner calls `CancellationSource.cancel()`.

```dream
let src = CancellationSource();
let tok = src.token;
let w = Task.spawn(() => {
    while !tok.is_cancelled { /* work */ }
    return 0;
});
src.cancel();
let _ = w.await;
```

**Hard abort:** `Promise.cancel(w)` (or dropping the Future) stops the task immediately. The browser terminates the worker; native `dream run` detaches the OS thread if the body is still running. Hard abort does **not** run Dream `finally` and may abandon that task's private memory — prefer the token when `@shared` state must stay consistent.

A task that has already finished its body is joined and its env is released (`Debug.live_objects` stays flat across repeated `spawn`).

## Async task bodies

A task body may `await` via `spawn_async` (named `async fun` or `async` lambda):

```dream
async fun main(): void {
    let n = 6;
    let squarer = Task.spawn_async(async () => {
        Time.sleep(1).await;
        return n * n;
    });
    System.println((squarer.await).to_string()); // 36
}
```

## `TaskPool` — reuse threads (advanced)

For many short jobs over time, a `TaskPool` keeps a fixed set of threads and **dispatches** work round-robin.
Prefer `spawn` / `map` for typical one-shot parallelism; reach for a pool when spawn/teardown cost dominates.

```dream
async fun main(): void {
    let pool = TaskPool(4);
    let a = pool.dispatch(() => 3 * 3);
    System.println((a.await).to_string()); // 9
    pool.shutdown();
}
```

Capture rules match `spawn`.
Async bodies use `dispatch_async`.

## Runtimes

| Runtime | Notes |
|---------|--------|
| Native (`dream run`) | One OS thread per task; private memory per task plus shared objects for `@shared`. |
| Browser | One `Worker` per task; private memory per task; `@shared` needs `Cross-Origin-Opener-Policy` / `Cross-Origin-Embedder-Policy` (shared memory). |
| Node | One `worker_threads.Worker` per task; same private-vs-shared memory model. |

## Notes and limits

- Body is `fun(): TOut` (`spawn`) or `fun(): Future<TOut>` (`spawn_async`).
- `TOut` must be `shared`. Captures must be `shared` **or moved** (ordinary arrays / `List` / classes transfer ownership; see [Sharing state safely](#sharing-state-safely)).
- A moved or captured heap value is visible to both sides while both can observe it. Task-local `new` / strings stay in that task's private memory.
- `T : shared` is the generic kind constraint (same family as `T : unmanaged`).

## See also

- [Lock & Semaphore](../stdlib/sync.md) — standalone synchronization primitives.
- [Classes & Structs](classes-structs.md) — `@shared class` and the closed-graph field rule.
- [Memory Management](memory.md) — ARC, including the path `@shared` classes use.
- [Async](async.md) — cooperative `CancellationToken` / `Promise.cancel`.
- [Generics](generics.md) — `T : shared`.

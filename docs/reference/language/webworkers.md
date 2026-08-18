# WebWorkers

**Package:** `system.core` (bootstrap — no import required for `WebWorker` / `WebWorkerPool`). Console examples below also use `import system;`.

Dream's [`async`/`await`](async.md) is a *single-threaded* scheduler: tasks interleave at `await` points but never run at the same instant. When you need more than one core — CPU-bound work or parallel pipelines — use a **`WebWorker`**.

`await WebWorker.spawn(() => …)` starts a body on its own OS thread (native) or Web Worker (browser) and waits for the result — the same shape as C# `Task.Run` / Swift `Task { }`. There is no spawn message and no `join()`. Each worker has its own private globals, but **shares the same heap memory** with the owner. Heap objects (`@shared class`, `Lock` / `Semaphore`, `CancellationToken`) are visible across workers — real parallelism with shared state, guarded by `@shared` / `lock`.

Captures and the body's return type must be **`shared`** (Dream's Sendable analogue): a blittable value, `string`, a value struct of `shared` fields, or an `@shared class`. Ordinary classes, arrays, and `List` are a compile error.

!!! note "Browser status"
    The browser runtime (`runtime/dream.js`) imports the same shared `WebAssembly.Memory` (`SharedArrayBuffer`) into every spawned `Worker`, matching native — but the host page must be served with the [Cross-Origin Isolation](https://developer.mozilla.org/en-US/docs/Web/API/crossOriginIsolated) headers (`Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp`, or `credentialless`) or `SharedArrayBuffer` allocation fails silently in some browsers. `@shared` capture across workers needs those headers.

## The model

```
┌────────────────────────────┐            ┌────────────────────────────┐
│      Owner instance        │            │      Worker instance       │
│                            │            │                            │
│  await WebWorker.spawn(    │── env ────►│       body(): TOut         │
│        () => body)         │  captures  │    (starts immediately)    │
│                            │            │                            │
│                            │◄─ result ──│                            │
│                            │ wire copy  │                            │
└─────────────┬──────────────┘            └──────────────┬─────────────┘
              │                                          │
              └──────────────────┬───────────────────────┘
                                 ▼
                ┌────────────────────────────────────────┐
                │         Shared linear memory           │
                │  @shared mutations visible both sides  │
                └────────────────────────────────────────┘
```

- **The worker body is a function value** — a top-level function or a lambda. Its function-table index is portable across every instance of the module.
- **Captures must be `shared`.** Overlap work by not awaiting yet: `let a = WebWorker.spawn(...); let b = WebWorker.spawn(...); await a; await b;`.

## API

```dream
public class WebWorker {
    public static async fun spawn<TOut : shared>(body: fun(): TOut): TOut;
    public static async fun spawn_async<TOut : shared>(body: fun(): Future<TOut>): TOut;
    public static async fun map<T : shared, TOut : shared>(items: T[], body: fun(T): TOut): TOut[];
    public static async fun map_async<T : shared, TOut : shared>(items: T[], body: fun(T): Future<TOut>): TOut[];
}
```

`TOut` is inferred from the body. Cancelling or dropping the spawn Future hard-aborts the worker (`Promise.cancel`). Prefer a captured `@shared` `CancellationToken` for cooperative cancel.

## Spawn

```dream
import system;

fun greet(name: string): string {
    return "hello, " + name + "!";
}

async fun main(): void {
    let name = "dream";
    System.println(await WebWorker.spawn(() => greet(name)));   // hello, dream!

    let n = 6;
    System.println((await WebWorker.spawn(() => n * n)).to_string()); // 36
}
```

## Running work in parallel

Save the Futures before the first `await` so they compute concurrently:

```dream
fun work(input: string): string {
    let i = 0;
    while (i < 5000000) { i = i + 1; }
    return input.to_upper();
}

async fun main(): void {
    let w1 = WebWorker.spawn(() => work("alpha"));
    let w2 = WebWorker.spawn(() => work("beta"));
    let w3 = WebWorker.spawn(() => work("gamma"));

    System.println(await w1);   // ALPHA
    System.println(await w2);   // BETA
    System.println(await w3);   // GAMMA
}
```

For the common "run N independent computations and collect the results" shape, `WebWorker.map` (below) does this without manual bookkeeping.

## `WebWorker.map` — parallel map

`WebWorker.map` fans a body out over an array in parallel and collects replies **in input order** — one worker per element. The array stays on the owner; each element is a `shared` value copied into that task:

```dream
fun square(x: string): string {
    let n = int.parse(x).unwrap_or(0);
    return (n * n).to_string();
}

async fun main(): void {
    let items = ["1", "2", "3", "4", "5"];
    let results = await WebWorker.map(items, square);
    for (let r in results) {
        System.println(r);   // 1, 4, 9, 16, 25
    }
}
```

The `body` argument follows the same capture rules as `spawn`.

## Sharing state safely

A worker body may be a **capturing lambda** — as long as everything it captures is `shared`:

- a blittable / unmanaged local,
- a `string`,
- a value struct of `shared` fields, or
- an **`@shared class`** instance.

Ordinary managed captures are a compile-time error — mark the class `@shared`, or pass blittable/`string` pieces.

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

    let a = WebWorker.spawn(() => { counter.increment(); return 0; });
    let b = WebWorker.spawn(() => { counter.increment(); return 0; });

    await a;
    await b;

    System.println(counter.value);   // 2
}
```

`lock (obj) { ... }` is a reentrant mutual-exclusion block and requires an `@shared class` (a lock word), not every `shared` type. An `@shared class`'s fields must themselves be `shared` (the closed-graph rule).

## Cancellation

Two layers (same split as C# / asyncio):

**Cooperative (preferred):** capture an `@shared` `CancellationToken` and poll it; the owner calls `CancellationSource.cancel()`.

```dream
let src = CancellationSource();
let tok = src.token();
let w = WebWorker.spawn(() => {
    while (!tok.is_cancelled()) { /* work */ }
    return 0;
});
src.cancel();
let _ = await w;
```

**Hard abort:** `Promise.cancel(w)` (or dropping the Future) stops the worker immediately. The browser terminates the worker; native `dream run` does the same. Hard abort does **not** run Dream `finally` — prefer the token when `@shared` state must stay consistent.

## Async worker bodies

A worker body may `await` via `spawn_async` (named `async fun` or `async` lambda):

```dream
async fun main(): void {
    let n = 6;
    let squarer = WebWorker.spawn_async(async () => {
        await Time.sleep(1);
        return n * n;
    });
    System.println((await squarer).to_string()); // 36
}
```

## `WebWorkerPool` — reuse worker threads (advanced)

For many short jobs over time, a `WebWorkerPool` keeps a fixed set of threads and **dispatches** work round-robin. Prefer `spawn` / `map` for typical one-shot parallelism; reach for a pool when spawn/teardown cost dominates.

```dream
async fun main(): void {
    let pool = WebWorkerPool(4);
    let a = pool.dispatch(() => 3 * 3);
    System.println((await a).to_string()); // 9
    pool.shutdown();
}
```

Capture rules match `spawn`. Async bodies use `dispatch_async`.

## Runtimes

| Runtime | Notes |
|---------|--------|
| Native (`dream run`) | One OS thread per worker; shared heap with the owner. |
| Browser | One `Worker` per worker; shared memory needs COOP/COEP on the host page. |
| Node | One `worker_threads.Worker` per worker; same shared-memory model. |

## Notes and limits

- Body is `fun(): TOut` (`spawn`) or `fun(): Future<TOut>` (`spawn_async`).
- `TOut` and captures must be `shared`. Arrays / `List` / ordinary classes are not.
- `T : shared` is the generic kind constraint (same family as `T : unmanaged`).

## See also

- [Lock & Semaphore](../stdlib/sync.md) — standalone synchronization primitives.
- [Classes & Structs](classes-structs.md) — `@shared class` and the closed-graph field rule.
- [Memory Management](memory.md) — ARC, including the atomic path `@shared` classes use.
- [Async](async.md) — cooperative `CancellationToken` / `Promise.cancel`.
- [Generics](generics.md) — `T : shared`.

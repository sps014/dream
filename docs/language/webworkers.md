# WebWorkers

**Package:** `system.core` (bootstrap — no import required for `WebWorker` / `WebWorkerPool`). Console examples below also use `import system;`.

Dream's [`async`/`await`](async.md) is a *single-threaded* scheduler: tasks interleave at `await` points but never run at the same instant. When you need more than one core — CPU-bound work or parallel pipelines — use a **`WebWorker`**.

A `WebWorker<TIn, TOut>` starts a body on its own OS thread (native) or Web Worker (browser) via **`spawn`**, then **`await join()`** for the single result — the same shape as C# `Task.Run` / Python `asyncio.to_thread`. Each worker has its own private globals, but **shares the same heap memory** with the owner. Heap objects (`@shared class`, `Lock` / `Semaphore`, `CancellationToken`) are visible across workers — real parallelism with shared state, guarded by `@shared` / `lock`.

Memory is a **cooperative stop-the-world generational GC** on that shared linear memory (not isolated per-worker heaps): a collection takes the allocator lock and waits until every live instance has reached a safepoint before evacuating. See [Memory Management](memory.md) and [`docs/compiler/12-tiered-gc.md`](../compiler/12-tiered-gc.md).

Optional wire arguments / `TOut` must each be `string`, an `unmanaged` (blittable) value type, or a `T[]` array of one: values cross the thread boundary on an internal wire format — never a live non-`@shared` pointer.

!!! note "Browser status"
    The browser runtime (`runtime/dream.js`) imports the same shared `WebAssembly.Memory` (`SharedArrayBuffer`) into every spawned `Worker`, matching native — but the host page must be served with the [Cross-Origin Isolation](https://developer.mozilla.org/en-US/docs/Web/API/crossOriginIsolated) headers (`Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp`, or `credentialless`) or `SharedArrayBuffer` allocation fails silently in some browsers. Plain wire spawn/join works without those headers; `@shared` capture across workers does not.

## The model

```
 Owner instance                            Worker instance
 ─────────────                             ───────────────
 WebWorker.spawn(arg, body) ── wire copy ──▶ body(arg): TOut   (starts immediately)
 await w.join()             ◀── wire copy ── result

              (native) same shared linear memory underneath —
              an @shared object mutated by one thread is visible to the other
```

- **The worker body is a function value** — a top-level function or a lambda. Its function-table index is portable across every instance of the module.
- **A capturing lambda body may only capture `@shared class` instances or unmanaged/value locals.** Capturing an ordinary managed object is a compile-time error — it is not in worker-shared memory.

## API

```dream
public class WebWorker<TIn, TOut> {
    public static fun spawn(input: TIn, body: fun(TIn): TOut): WebWorker<TIn, TOut>;
    public static fun spawn(input: TIn, body: fun(TIn): Future<TOut>): WebWorker<TIn, TOut>;
    public static fun spawn(body: fun(): TOut): WebWorker<TIn, TOut>;
    public static fun spawn(body: fun(): Future<TOut>): WebWorker<TIn, TOut>;

    public async fun join(): TOut;
    public fun terminate(): void;

    public static async fun map(items: TIn[], body: fun(TIn): TOut): TOut[];
}
```

## Spawn / join

```dream
import system;

fun greet(name: string): string {
    return "hello, " + name + "!";
}

async fun main(): void {
    let w = WebWorker<string, string>.spawn("dream", greet);
    System.println(await w.join());   // hello, dream!
    w.terminate();

    let squarer = WebWorker<int, int>.spawn(6, (n) => n * n);
    System.println((await squarer.join()).to_string()); // 36
    squarer.terminate();
}
```

## Running work in parallel

Spawn several workers before the first `await join` so they compute concurrently:

```dream
fun work(input: string): string {
    let i = 0;
    while (i < 5000000) { i = i + 1; }
    return input.to_upper();
}

async fun main(): void {
    let w1 = WebWorker<string, string>.spawn("alpha", work);
    let w2 = WebWorker<string, string>.spawn("beta", work);
    let w3 = WebWorker<string, string>.spawn("gamma", work);

    System.println(await w1.join());   // ALPHA
    System.println(await w2.join());   // BETA
    System.println(await w3.join());   // GAMMA

    w1.terminate();
    w2.terminate();
    w3.terminate();
}
```

For the common "run N independent computations and collect the results" shape, `WebWorker.map` (below) does this without manual bookkeeping.

## `WebWorker.map` — parallel map

`WebWorker<TIn, TOut>.map` fans a body out over an array in parallel and collects replies **in input order** — one worker per element, spawn-then-join:

```dream
fun square(x: string): string {
    let n = int.parse(x).unwrap_or(0);
    return (n * n).to_string();
}

async fun main(): void {
    let items = ["1", "2", "3", "4", "5"];
    let results = await WebWorker<string, string>.map(items, square);
    for (let r in results) {
        System.println(r);   // 1, 4, 9, 16, 25
    }
}
```

The `body` argument follows the same capture rules as `spawn`.

## Sharing state safely

A worker body may be a **capturing lambda** — as long as everything it captures is safe to touch from another thread:

- an **`@shared class`** instance, or
- an **unmanaged/value** local (snapshotted by value).

Ordinary managed captures are a compile-time error: the type is not in worker-shared memory — mark it `@shared`, pass it as a wire `spawn` argument, or allocate it inside the worker body.

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

    let a = WebWorker<string, string>.spawn("go", (msg) => { counter.increment(); return msg; });
    let b = WebWorker<string, string>.spawn("go", (msg) => { counter.increment(); return msg; });

    await a.join();
    await b.join();

    System.println(counter.value);   // 2
    a.terminate();
    b.terminate();
}
```

`lock (obj) { ... }` is a reentrant mutual-exclusion block. An `@shared class`'s fields must themselves be unmanaged/value types or other `@shared class` instances (the closed-graph rule).

## Cancellation

Two layers (same split as C# / asyncio):

**Cooperative (preferred):** capture an `@shared` `CancellationToken` and poll it; the owner calls `CancellationSource.cancel()`.

```dream
let src = CancellationSource();
let tok = src.token();
let w = WebWorker<int, int>.spawn(() => {
    while (!tok.is_cancelled()) { /* work */ }
    return 0;
});
src.cancel();
let _ = await w.join();
```

**Hard abort:** `w.terminate()` (and `del`) kills the worker thread. Browser uses `Worker.terminate()`; native uses wasmtime epoch interruption so a busy body aborts. Pending `join()` settles (does not hang). Hard abort does **not** run Dream `finally` / orderly teardown — prefer the token when `@shared` state must stay consistent.

## Async worker bodies

A worker body may `await` (named `async fun` or `async` lambda):

```dream
async fun main(): void {
    let squarer = WebWorker<int, int>.spawn(6, async (n) => {
        await Time.sleep(1);
        return n * n;
    });
    System.println((await squarer.join()).to_string()); // 36
    squarer.terminate();
}
```

## `WebWorkerPool` — reuse worker threads (advanced)

For many short jobs over time, a `WebWorkerPool` keeps a fixed set of threads and **dispatches** work round-robin. Prefer `spawn`/`join`/`map` for typical one-shot parallelism; reach for a pool when spawn/teardown cost dominates.

```dream
async fun main(): void {
    let pool = WebWorkerPool(4);
    let a = pool.dispatch(3, (n) => n * n);
    System.println((await a).to_string()); // 9
    pool.shutdown();
}
```

Capture and wire rules match `spawn`.

## Runtimes

| Runtime | Notes |
|---------|--------|
| Native (`dream run`) | One OS thread per worker; shared heap + cooperative STW GC with the owner. |
| Browser | One `Worker` per worker; shared memory needs COOP/COEP on the host page. |
| Node | One `worker_threads.Worker` per worker; same shared-memory GC model. |

## Notes and limits

- Body is `fun(): TOut`, `fun(TIn): TOut`, or the `Future`-returning forms.
- Wire types must be `string`, unmanaged, or a `T[]` of one. Pass `@shared` state by capturing it, not as the message type.
- `terminate()` is idempotent and also runs when the handle is destroyed.

## See also

- [Lock & Semaphore](../stdlib/sync.md) — standalone synchronization primitives.
- [Classes & Structs](classes-structs.md) — `@shared class` and the closed-graph field rule.
- [Memory Management](memory.md) — generational GC, including the path `@shared` classes use.
- [Async](async.md) — cooperative `CancellationToken` / `Promise.cancel`.

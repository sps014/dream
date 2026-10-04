# Reuse workers with TaskPool

A pool keeps workers available for repeated jobs. Start with ordinary tasks unless your workload needs repeated dispatch.

[Back to overview](tasks.md)

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

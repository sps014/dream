---
title: Tasks
summary: Run CPU work in parallel on runtime workers.
icon: material/run-fast
status: stable
targets: [native, web, node]
related: [language/concurrency/structured.md, language/concurrency/sharing.md, language/concurrency/task-pool.md]
---

# Tasks

Use [structured concurrency](structured.md) for concurrent I/O and owned task lifetimes. Use `scope.spawn_parallel` for CPU work on shared runtime workers, or [TaskPool](task-pool.md) to bound parallelism explicitly.

!!! glance "At a glance"

    ```dream
    let values = Task.map<int, int, string>([1, 2, 3], compute, concurrency: 2).await;
    ```

    - Tasks run CPU work in parallel on runtime workers.
    - Use structured concurrency for I/O and owned lifetimes.

**Import:** `import system.task;`

```dream
import system;
import system.task;

async fun compute(value: int): Result<int, string> {
    Time.delay(1).await;
    return Result.Ok(value * value);
}

async fun main(): void {
    let values = Task.map<int, int, string>([1, 2, 3], compute, concurrency: 2).await.unwrap();
    for (let value in values) {
        System.println(value);
    }
}
```

`Task.scope` automatically joins children on success, failure, cancellation, and early error return. `scope.spawn` schedules a local async child immediately, without allocating a thread or worker. Local children share ordinary memory and take turns at suspension points. A CPU loop can starve this scheduler unless it yields or checks cancellation.

For CPU parallelism, `scope.spawn_parallel(body)` uses the shared runtime workers. A fixed `TaskPool` bounds concurrency, and `scope.spawn_on(pool, body)` retains structured ownership of its CPU jobs. Worker captures must satisfy Dream's [shared/move constraints](sharing.md); local async children do not cross that boundary.

## Low-level workers

`Task.spawn` and `Task.spawn_async` run one function on a worker and return lazy futures. Start them with `Promise.start`, await them, or pass them to a combinator. Merely constructing several futures does not start their work.

```dream
let first = Task.spawn(() => 6 * 7);
let second = Task.spawn(() => 7 * 8);
Promise.start(first);
Promise.start(second);
System.println(first.await);
System.println(second.await);
```

These low-level calls do not create a structured region. Prefer scopes and reusable pools for application work. Explicit cancellation tokens are available for cooperative worker logic; cancelling a scope requests cleanup instead of forcibly interrupting its children.

Browser workers sharing values require cross-origin isolation headers. Ordinary local async scopes do not require worker support. See [sharing state](sharing.md), [cancellation](cancellation.md), and [pool ownership](task-pool.md).

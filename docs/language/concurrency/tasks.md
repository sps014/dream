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

## `Task` functions

| Function | Returns | Runs |
|----------|---------|------|
| `Task.scope(body, timeout_ms, token)` | `Result<T, TaskFailure<E>>` | `body` with a new `TaskScope`, then joins every child; `timeout_ms` of `0` means no deadline |
| `Task.map(items, body, concurrency, token)` | `Result<U[], TaskFailure<E>>` | An async `body` per item, at most `concurrency` at once, with results in input order |
| `Task.for_each(items, body, concurrency, token)` | `Result<Unit, TaskFailure<E>>` | Like `map`, but discards the successful values |
| `Task.spawn(body, token)` | `T` | A synchronous `body` on its own worker, as a lazy future |
| `Task.spawn_async(body, token)` | `T` | An async `body` on its own worker, as a lazy future |

All of these are `async`, so you `await` their result. `timeout_ms` and `token` are optional.

## `TaskScope` methods

| Member | Returns | Effect |
|--------|---------|--------|
| `scope.spawn(body)` | `TaskHandle<T, E>` | Starts a local async child; `body` returns `Future<Result<T, E>>` |
| `scope.spawn_parallel(body)` | `TaskHandle<T, E>` | Runs synchronous CPU work on the shared runtime workers |
| `scope.spawn_on(pool, body)` | `TaskHandle<T, E>` | Runs synchronous CPU work on a `TaskPool` |
| `scope.cancel()` | — | Requests cooperative cancellation of the scope and its children |
| `scope.token` | `CancellationToken` | The token the scope's children inherit |
| `handle.join()` | `Future<Result<T, TaskFailure<E>>>` | Waits for that child without cancelling it |

Spawning on a scope that is already cancelled or finished returns a handle whose result is `Err(TaskFailure.Cancelled)`.

## `TaskFailure` choices

| Choice | Meaning |
|--------|---------|
| `Failed(E)` | A child or the scope body returned an application error |
| `Cancelled` | The parent or caller requested cancellation |
| `TimedOut` | The scope deadline expired before all work completed |
| `WorkerFailed(TaskPoolError)` | A worker rejected a submitted job, or crashed before replying (`TaskPoolError.WorkerCrashed`) |
| `InvalidOperation` | An invalid limit, such as a `concurrency` of `0` or less or a negative `timeout_ms` |

## Worker crashes

On the web and on Node, a worker can stop before it replies. Its job may trap (a failed `unwrap`, an out-of-range index, an explicit `System.panic`), the worker thread may exit, a message may fail to reach it, or the worker may not start within 10 seconds. The program keeps running, and the crash becomes an ordinary error:

- `pool.dispatch` and `pool.dispatch_async` return `Err(TaskPoolError.WorkerCrashed)`. A job queued on the same worker gets the same error. The pool then starts a replacement worker, so later dispatches run normally.
- `scope.spawn_parallel` and `scope.spawn_on` children finish with `Err(TaskFailure.WorkerFailed(TaskPoolError.WorkerCrashed))`, and the scope is cancelled as for any other worker failure.
- `Task.spawn` and `Task.spawn_async` return the value directly, so they have no error to return. They panic with the worker's failure message instead.

```dream
import system;
import system.task;

fun checked_square(value: int): int {
    if value < 0 {
        System.panic("negative input");
    }
    return value * value;
}

async fun main(): void {
    let pool = TaskPool(2);
    let result = pool.dispatch(() => checked_square(-1)).await;
    if result.is_err() {
        System.println(result.unwrap_err().message());
    }
    pool.close().await.unwrap();
}
```

This prints `task worker crashed before replying`. The crashed job's result is lost, and objects it held when it stopped stay allocated.

On native targets, workers are threads inside your process, so a trapping job ends the whole program, just as a trap on the main thread does.

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

---
title: Task pools
summary: Reuse a fixed set of workers for parallel CPU jobs.
icon: material/pool
status: stable
targets: [native, web, node]
---

# Reuse workers with TaskPool

Use a pool for CPU work that should execute in parallel. Use [structured local async children](structured.md) for concurrent I/O.

!!! glance "At a glance"

    ```dream
    let pool = TaskPool(4);
    let result = pool.dispatch(() => 3 * 3).await.unwrap();
    pool.close().await.unwrap();
    ```

    - A pool reuses a fixed number of workers for parallel CPU jobs.
    - Close the pool when you no longer need it.

```dream
import system;
import system.task;

async fun main(): void {
    let pool = TaskPool(4);
    let result = pool.dispatch(() => 3 * 3).await.unwrap();
    System.println(result);
    let values = pool.map<int, int>([1, 2, 3], (value) => value * value).await.unwrap();
    pool.close().await.unwrap();
}
```

`dispatch` accepts synchronous worker code; `dispatch_async` accepts an async callback. Both return `Result<T, TaskPoolError>`. They check cancellation before submission and after the job settles. Async worker jobs inherit the submitted token. Long synchronous CPU jobs must capture and check a token explicitly to stop early.

`map` and `map_async` keep at most `size` dispatches active and preserve input order. Their failures use `TaskFailure<TaskPoolError>`. Worker captures and result types follow the [sharing rules](sharing.md).

`close().await` stops accepting work, waits for active dispatches, and closes workers. Repeated closure succeeds. A nonpositive pool size creates no workers and dispatch returns `InvalidSize`; dispatch after closure returns `Closed`. A constructed dispatch future is lazy, so close before starting it rejects its later submission.

Use `scope.spawn_on(pool, body)` when a region should own a CPU job alongside local async work. Native worker replies wake the owner scheduler instead of blocking it.

## `TaskPool` members

| Member | Returns | Effect |
|--------|---------|--------|
| `TaskPool(size)` | `TaskPool` | Starts `size` reusable workers |
| `.size` | `int` | Number of workers in the pool |
| `.dispatch(body, token)` | `Future<Result<T, TaskPoolError>>` | Runs a synchronous `body` on a worker |
| `.dispatch_async(body, token)` | `Future<Result<T, TaskPoolError>>` | Runs an async `body` on a worker |
| `.map(items, body, token)` | `Future<Result<U[], TaskFailure<TaskPoolError>>>` | Runs a synchronous `body` per item, at most `size` at once, in input order |
| `.map_async(items, body, token)` | `Future<Result<U[], TaskFailure<TaskPoolError>>>` | Same as `map`, with an async `body` |
| `.close()` | `Future<Result<Unit, TaskPoolError>>` | Stops accepting work, waits for active jobs, and shuts the workers down |

The `token` argument is optional; without it, the pool uses the token inherited from the enclosing task scope.

## `TaskPoolError` choices

| Choice | Returned when |
|--------|---------------|
| `InvalidSize` | The pool was created with a size of `0` or less |
| `Closed` | Work was submitted after `close()` |
| `Cancelled` | Cancellation was requested; any submitted work has settled first |

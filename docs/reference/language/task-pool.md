# Reuse workers with TaskPool

Use a pool for CPU work that should execute in parallel. Use [structured local async children](structured-concurrency.md) for concurrent I/O.

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

`map` and `map_async` keep at most `size` dispatches active and preserve input order. Their failures use `TaskFailure<TaskPoolError>`. Worker captures and result types follow the [sharing rules](tasks-sharing.md).

`close().await` stops accepting work, waits for active dispatches, and closes workers. Repeated closure succeeds. A nonpositive pool size creates no workers and dispatch returns `InvalidSize`; dispatch after closure returns `Closed`. A constructed dispatch future is lazy, so close before starting it rejects its later submission.

Use `scope.spawn_on(pool, body)` when a region should own a CPU job alongside local async work. Native worker replies wake the owner scheduler instead of blocking it.

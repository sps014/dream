---
title: Structured concurrency
summary: "Own concurrent work with `Task.scope` so nothing outlives its parent."
icon: material/file-tree-outline
status: stable
targets: [native, web, node]
related: [language/concurrency/async.md, language/concurrency/cancellation.md, language/concurrency/tasks.md]
---

# Structured concurrency

Use `Task.scope` to own concurrent work. Local async children share the current scheduler and ordinary memory, and they do not start an OS thread, Node worker, or browser worker.

!!! glance "At a glance"

    ```dream
    let result = Task.scope<int, string>(
        async (scope) => {
            let child = scope.spawn<int>(async () => load(42).await);
            return child.join().await;
        }
    ).await;
    ```

    - `Task.scope` owns every child it starts; none outlives the scope.
    - Local async children share the current scheduler and need no OS thread.

```dream
import system;
import system.task;

async fun load(value: int): Result<int, string> {
    Time.delay(1).await;
    return Result.Ok(value);
}

async fun main(): void {
    let result = Task.scope<int, string>(
        async (scope) => {
            let first = scope.spawn<int>(async () => load(20).await);
            let second = scope.spawn<int>(async () => load(22).await);
            return Result.Ok(first.join().await? +second.join().await?);
        }
    ).await;
    System.println(result.unwrap());
}
```

`spawn` starts its child immediately, and dropping the handle does not cancel it. `join` waits for the result, and later joins see that same result.

After the body returns, the scope accepts no new children and joins the ones it already started. A handle may leave the scope to read a finished result, but it cannot start more work.

A child error, or an error from the scope body, cancels the other children. A successful return waits for unfinished children without cancelling them, and the scope finishes cleanup before it returns.

## Cancellation and errors

`TaskFailure<E>` keeps the original application error as `Failed(E)`. Other outcomes are `Cancelled`, `TimedOut`, `WorkerFailed(TaskPoolError)`, and `InvalidOperation`.

The first child or worker failure is the one reported. A body error is returned after cleanup only when no child failed first, and a deadline or parent cancellation does not interrupt a CPU loop.

```dream
let result = Task.scope<int, string>(
    async (scope) => {
        let child = scope.spawn<int>(async () => load(42).await);
        return child.join().await;
    },
    timeout_ms: 5000,
    token: Option.Some(cancellation.token)
).await;
```

Children inherit the scope token, available as `scope.token` or `CancellationToken.current`. Async file, HTTP, timer, process, channel, scoped task, and pool operations use that token when you omit it or pass `Option.None`.

An explicit token overrides it. To finish cleanup after the parent is cancelled, pass a token from a separate source that is still active.

`Time.sleep` and `Time.delay` return `Result<Unit, CancelledError>` and wake on cancellation. Check or propagate that result when later work depends on the timer finishing.

Cancelling a scope that is already running asks its children to stop and waits until they settle.

Local scheduling is cooperative: a CPU loop must yield or check cancellation, and a blocking foreign call can block its scheduler. A deadline asks for cancellation and does not promise that cleanup finishes by that time.

## Bounded mapping

```dream
let values = Task.map<int, int, string>(
    [1, 2, 3, 4],
    async (value) => load(value).await,
    concurrency: 32
).await?;
```

`Task.map` keeps input order and runs at most `concurrency` workers, not one child per input. A failure stops new inputs, cancels active siblings, and waits for them.

`concurrency` must be positive, including for an empty input. `Task.for_each` works the same way and discards successful values.

The input array remains held during execution. Bounds on active operations do not constrain memory allocated by each operation or by application code.

## CPU parallelism

CPU work uses the runtime's shared workers by default:

```dream
let result = Task.scope<int, string>(
    async (scope) => {
        let first = scope.spawn_parallel<int>(() => 6 * 7);
        return Result.Ok(first.join().await?);
    }
).await;
```

`spawn_parallel` starts immediately and keeps the job until the scope finishes, even if you drop the handle. Cancellation waits for submitted work to finish.

Native threads and Node or browser workers are reused. The shared runtime may start one worker per active job, so use a `TaskPool` to cap CPU concurrency:

```dream
let pool = TaskPool(4);
let values = pool.map<int, int>([1, 2, 3], (value) => value * value).await?;
pool.close().await?;
```

`scope.spawn_on(pool, body)` returns a normal join handle. Worker captures and results follow the `shared` and move rules.

Check the scope token inside a long synchronous CPU job. An async pool job inherits the token you submit, and closing a pool stops new work, waits for active jobs, and is safe to repeat.

## Channels and backpressure

```dream
let channel = Channel.bounded<string>(capacity: 256)?;
let sender = channel.sender;
let receiver = channel.receiver;

sender.send("hello").await?;
sender.close();
let item = receiver.receive().await?;
```

A bounded channel returns `InvalidCapacity` unless capacity is positive.

- A full `send` waits. `try_send` returns `Full`, and `try_receive` returns `Empty` while more values may arrive.
- `receive` returns `None` only after every sender closes and the queue drains.
- A closed endpoint returns `Closed`. A cancelled wait returns `Cancelled` and does not undo a send or receive that already finished.

Each endpoint getter creates an independent endpoint. Closing the last sender closes sends, and closing the last receiver drops queued values and wakes blocked sends.

Aliases of one endpoint share its close state. Keep the receiving endpoint alive while producers run.

Local channels accept ordinary values, and `Channel.shared_bounded<T>` moves `shared` values across workers. Its endpoints are shared classes: `clone()` makes an independent endpoint, and an ordinary alias shares close state.

`unbounded` and `shared_unbounded` have no capacity limit. Channel capacity bounds item count, not item size, so a queue of large buffers can still use a lot of memory.

## Control memory use

Limit how many children run at once and how much data each one keeps. Suspended work still holds its captured values and unfinished results.

A channel capacity limits how many items are queued, so also limit each item's size. Use a bounded channel when producers can get ahead of consumers.

Close unused endpoints, and finish or cancel scopes when the work is done. Measure memory and latency with realistic payloads before you pick task counts and queue sizes.

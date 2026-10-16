# Structured concurrency

Use `Task.scope` to own concurrent work. Local async children share the current scheduler and ordinary memory. They do not allocate an OS thread, Node worker, or browser worker.

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

`spawn` schedules its child immediately. A discarded handle does not cancel the child. `join` waits for its result, and multiple joins observe the same result. After the body returns, the scope stops accepting children and joins every previously accepted child. A handle can escape to inspect its completed result; neither it nor an escaped scope can start work after the region finishes.

A child returning an application error cancels its siblings immediately. An early error from the scope body also cancels children. Successful return waits for unfinished children without cancelling them. The scope always waits for cleanup before exposing its outcome.

## Cancellation and errors

`TaskFailure<E>` preserves the original application error as `Failed(E)`. Other outcomes are `Cancelled`, `TimedOut`, `WorkerFailed(TaskPoolError)`, and `InvalidOperation`. The first application or worker failure that requests cancellation is retained. A body error is returned after cleanup when no child initiated failure first. Deadlines and parent cancellation never forcibly interrupt a CPU loop.

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

Children inherit the scope's token, available as `scope.token` or `CancellationToken.current`. Async file, HTTP, timer, process, channel, scoped task, and pool operations resolve an omitted token, or `Option.None`, to that context. An explicit token overrides it. To finish cleanup despite parent cancellation, pass a token from a separate, uncancelled source.

`Time.sleep` and `Time.delay` return `Result<bool, CancelledError>` and wake on cancellation. Check or propagate that result when subsequent work depends on the timer finishing. Cancelling an already running scope future requests cancellation and leaves the scope schedulable until its children settle.

Local scheduling is cooperative. A CPU loop must yield or check cancellation; a blocking foreign call can block its scheduler. A deadline requests cancellation rather than promising a strict upper bound on cleanup time.

## Bounded mapping

```dream
let values = Task.map<int, int, string>(
    [1, 2, 3, 4],
    async (value) => load(value).await,
    concurrency: 32
).await?;
```

`Task.map` preserves input order and creates at most `concurrency` worker loops, rather than a child for every input. A failure stops taking new inputs, cancels active siblings, and joins them. Positive concurrency is required, including for an empty input. `Task.for_each` has the same behavior but discards successful values and allocates no per-input result storage.

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

`spawn_parallel` starts immediately and retains the job until the scope drains, even
if its handle is discarded. Cancellation waits for submitted work to settle. Native
threads and Node/browser workers are reused across completed jobs; the shared runtime
may create a worker for each active job. Use an explicit `TaskPool` to bound CPU concurrency:

```dream
let pool = TaskPool(4);
let values = pool.map<int, int>([1, 2, 3], (value) => value * value).await?;
pool.close().await?;
```

Inside a scope, `scope.spawn_on(pool, body)` returns a normal join handle for a CPU job. Worker capture and result types follow the existing `shared` and move rules. Long synchronous CPU jobs should capture the scope token and check it. Async pool jobs inherit the submitted token. Pool closure stops new submissions, waits for active work, and closes workers idempotently.

## Channels and backpressure

```dream
let channel = Channel.bounded<string>(capacity: 256)?;
let sender = channel.sender;
let receiver = channel.receiver;

sender.send("hello").await?;
sender.close();
let item = receiver.receive().await?;
```

A bounded factory returns `InvalidCapacity` unless capacity is positive. When full, `send` suspends. `try_send` returns `Full`; `try_receive` returns `Empty` while more values could arrive. `receive` returns `None` only after all senders close and queued items drain. Closed endpoints return `Closed`; cancelled waits return `Cancelled` without rolling back a completed send or receive.

Each endpoint getter creates an independently owned endpoint. Closing or dropping the last sender closes sends; closing or dropping the last receiver discards queued values and wakes blocked sends. Aliases refer to the same endpoint. Keep the receiving endpoint alive while producers run.

Local channels accept ordinary values. Use `Channel.shared_bounded<T>` to transfer `shared` values across workers. Its endpoints are shared classes; their `clone()` methods create independent endpoints, while ordinary aliases share the same close state. Shared channels use synchronized queues and cross-worker wakeups. `unbounded` and `shared_unbounded` explicitly opt out of capacity backpressure.

Channel capacity bounds item count, not item size. A queue of large buffers can still use substantial memory.

## Control memory use

Limit both the number of active children and the amount of data each child retains.
Suspended work still keeps its captured values and unfinished results alive. Channel
capacity bounds the number of queued items, so also bound the size of each item.

Use a bounded channel when producers may outrun consumers. Close unused endpoints and
finish or cancel scopes promptly to release work that is no longer needed. Measure
memory and latency with realistic payloads before choosing task counts and queue sizes.

# TaskPool

**Import:** `import system.task;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `TaskPool`

Reuses a fixed number of parallel workers; local async tasks do not consume pool workers. Each worker holds at most two jobs. Further dispatches wait in submission order and go to the least-loaded worker once one finishes, so a long job holds up at most one job behind it.

```dream
public class TaskPool
```

## `constructor`

Creates a fixed worker set; nonpositive sizes produce InvalidSize on operations.

```dream
public constructor(size: int)
```

## `size`

Number of reusable parallel workers in this pool.

```dream
public get size(): int
```

## `dispatch`

Dispatches a CPU job; capture and return values follow the shared/move worker rules.

```dream
public fun dispatch<TOut: shared>(
    body: fun(): TOut,
    token: Option<CancellationToken> = Option.None
): Future<Result<TOut, TaskPoolError>>
```

## `dispatch_async`

Dispatches an async job on a pool worker and waits for its operation to settle.

```dream
public fun dispatch_async<TOut: shared>(
    body: fun(): Future<TOut>,
    token: Option<CancellationToken> = Option.None
): Future<Result<TOut, TaskPoolError>>
```

## `map`

Maps CPU work with at most size active dispatches and preserves input order.

```dream
public fun map<T: shared, U: shared>(
    items: T[],
    body: fun(T): U,
    token: Option<CancellationToken> = Option.None
): Future<Result<U[], TaskFailure<TaskPoolError>>>
```

## `map_async`

Maps async worker jobs with bounded dispatch rather than creating one worker per input.

```dream
public fun map_async<T: shared, U: shared>(
    items: T[],
    body: fun(T): Future<U>,
    token: Option<CancellationToken> = Option.None
): Future<Result<U[], TaskFailure<TaskPoolError>>>
```

## `close`

Stops accepting work, waits for submitted jobs, then closes workers idempotently.

```dream
public fun close(): Future<Result<Unit, TaskPoolError>>
```

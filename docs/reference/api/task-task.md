# Task

**Import:** `import system.task;`

Read the [structured concurrency guide](../language/structured-concurrency.md) for lifetimes, cancellation, backpressure, and worker execution.

## `Task`

Structured local async scopes and bounded maps, with explicit low-level worker spawning.

```dream
public class Task
```

## `scope`

Runs a structured async region and joins all children on success, failure, or cancellation.

```dream
public static async fun scope<T, E>(body: fun(TaskScope<E>): Future<Result<T, TaskFailure<E>>>, timeout_ms: int = 0, token: Option<CancellationToken> = Option.None): Result<T, TaskFailure<E>>
```

## `spawn`

Runs one synchronous function on a separate worker; use scope.spawn_on for structured CPU jobs.

```dream
public static async fun spawn<TOut : shared>(body: fun(): TOut, token: Option<CancellationToken> = Option.None): TOut
```

## `spawn_async`

Runs an async function on a separate worker; local async scope children need no worker.

```dream
public static async fun spawn_async<TOut : shared>(body: fun(): Future<TOut>, token: Option<CancellationToken> = Option.None): TOut
```

## `map`

Maps with bounded local async concurrency and returns values in input order.

```dream
public static async fun map<T, U, E>(items: T[], body: fun(T): Future<Result<U, E>>, concurrency: int, token: Option<CancellationToken> = Option.None): Result<U[], TaskFailure<E>>
```

## `for_each`

Applies an async operation with bounded concurrency; successful child values are discarded.

```dream
public static async fun for_each<T, U, E>(items: T[], body: fun(T): Future<Result<U, E>>, concurrency: int, token: Option<CancellationToken> = Option.None): Result<bool, TaskFailure<E>>
```

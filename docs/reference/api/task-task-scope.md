# Task Scope

**Import:** `import system.task;`

Read the [structured concurrency guide](../language/structured-concurrency.md) for lifetimes, cancellation, backpressure, and worker execution.

## `TaskHandle`

A child result whose execution is owned by a scope, independent of this handle's lifetime.

```dream
public class TaskHandle<T, E>
```

## `join`

Waits for completion without cancelling or transferring ownership of the child.

```dream
public fun join(): Future<Result<T, TaskFailure<E>>>
```

## `TaskScope`

Owns async and parallel children; its enclosing Task.scope joins every child before returning.

```dream
public class TaskScope<E>
```

## `token`

The inherited cancellation token for this scope and its children.

```dream
public get token(): CancellationToken
```

## `cancel`

Requests cooperative cancellation; completion still waits for child cleanup.

```dream
public fun cancel(): void
```

## `spawn`

Schedules a child immediately and retains it even if its result handle is discarded.

```dream
public fun spawn<T>(body: fun(): Future<Result<T, E>>): TaskHandle<T, E>
```

## `spawn_parallel`

Schedules CPU work on the runtime's shared worker threads and retains structured ownership.

```dream
public fun spawn_parallel<T : shared>(body: fun(): T): TaskHandle<T, E>
```

## `spawn_on`

Schedules CPU work on an explicit reusable pool while retaining structured ownership.

```dream
public fun spawn_on<T : shared>(pool: TaskPool, body: fun(): T): TaskHandle<T, E>
```

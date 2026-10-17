# TaskScope

**Import:** `import system.task;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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
public fun spawn_parallel<T: shared>(body: fun(): T): TaskHandle<T, E>
```

## `spawn_on`

Schedules CPU work on an explicit reusable pool while retaining structured ownership.

```dream
public fun spawn_on<T: shared>(pool: TaskPool, body: fun(): T): TaskHandle<T, E>
```

# TaskPool

**Import:** `import system.task;`

Read the [usage guide](../language/tasks.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class TaskPool`

Reuse a fixed set of threads when you dispatch many short jobs. Work goes out round-robin.

```dream
public class TaskPool
```

## `constructor`

```dream
public constructor(size: int)
```

## `dispatch`

Run `body` on the next pool member. Capture rules match `Task.spawn`.

```dream
public async fun dispatch<TOut : shared>(body: fun(): TOut, token: Option<CancellationToken> = Option.None): TOut
```

## `dispatch_async`

```dream
public async fun dispatch_async<TOut : shared>(body: fun(): Future<TOut>, token: Option<CancellationToken> = Option.None): TOut
```

## `shutdown`

```dream
public fun shutdown(): void
```

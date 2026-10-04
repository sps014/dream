# Task

**Import:** `import system.task;`

Read the [usage guide](../language/tasks.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class Task`

Run a function on another thread. Await the result like any other Future.  async fun main(): void { let square = Task.spawn(() => 6 * 6).await; System.println(square.to_string()); }  `spawn` is itself `async`, so awaiting it *is* the join. Hold the Future to overlap work. What a body may capture is covered in `docs/reference/language/tasks.md`.

```dream
public class Task
```

## `spawn`

```dream
public static async fun spawn<TOut : shared>(body: fun(): TOut, token: Option<CancellationToken> = Option.None): TOut
```

## `spawn_async`

```dream
public static async fun spawn_async<TOut : shared>(body: fun(): Future<TOut>, token: Option<CancellationToken> = Option.None): TOut
```

## `map`

```dream
public static async fun map<T : shared, TOut : shared>(borrow items: T[], body: fun(T): TOut, token: Option<CancellationToken> = Option.None): TOut[]
```

## `map_async`

```dream
public static async fun map_async<T : shared, TOut : shared>(borrow items: T[], body: fun(T): Future<TOut>, token: Option<CancellationToken> = Option.None): TOut[]
```

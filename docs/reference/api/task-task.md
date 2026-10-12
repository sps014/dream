# Task

**Import:** `import system.task;`

Read the [usage guide](../language/tasks.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class Task`

Run a function on another thread. Await the result like any other Future.  async fun main(): void { let square = Task.spawn(() => 6 * 6).await; System.println(square.to_string()); }  `spawn` is itself `async`, so awaiting it *is* the join. Hold the Future to overlap work. What a body may capture is covered in `docs/reference/language/tasks.md`.

```dream
public class Task
```

## `funcidx`

Returns the worker invocation selector for a synchronous wire callback. Prefer `Task` APIs.

```dream
public static extern fun funcidx(borrow body: fun(string): string): int
```

## `funcidx_async`

Returns the worker invocation selector for an async wire callback. Prefer `Task` APIs.

```dream
public static extern fun funcidx_async(borrow body: fun(string): Future<string>): int
```

## `env`

Returns the closure-environment transport word for a synchronous worker callback. Prefer `Task` APIs.

```dream
public static extern fun env(borrow body: fun(string): string): long
```

## `env_async`

Returns the closure-environment transport word for an async worker callback. Prefer `Task` APIs.

```dream
public static extern fun env_async(borrow body: fun(string): Future<string>): long
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

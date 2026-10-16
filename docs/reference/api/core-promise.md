# Promise

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `static class Promise`

Promise utilities for async tasks.

```dream
public static class Promise
```

## `all_host`

Intrinsic behind `Promise.all`; use `Promise.all` with a `Future<T>[]` for typed results and cancellation checks.

```dream
public static extern async fun all_host<T>(borrow promises: T[]): T[]
```

## `any_host`

Intrinsic behind `Promise.any`; use `Promise.any` with a `Future<T>[]` for typed results and cancellation checks.

```dream
public static extern async fun any_host<T>(borrow promises: T[]): T
```

## `race_host`

Intrinsic behind `Promise.race`; use `Promise.race` with a `Future<T>[]` for typed results and cancellation checks.

```dream
public static extern async fun race_host<T>(borrow promises: T[]): T
```

## `all`

Waits for all promises in the array to resolve.

```dream
public static async fun all<T>(borrow promises: Future<T>[], token: Option<CancellationToken> = Option.None): T[]
```

## `any`

Waits for any promise in the array to resolve.

```dream
public static async fun any<T>(borrow promises: Future<T>[], token: Option<CancellationToken> = Option.None): T
```

## `race`

Returns the first promise to resolve or reject.

```dream
public static async fun race<T>(borrow promises: Future<T>[], token: Option<CancellationToken> = Option.None): T
```

## `cancel`

Cancels a future (marks it cancelled and unlinks pending timers). The future it is awaiting is cancelled too when nothing else holds it. Best-effort for in-flight native host HTTP; cooperative `CancellationToken` is preferred for app logic. Takes the future as `object` (futures are heap refs) so this can be a plain intrinsic.

```dream
public static extern fun cancel(borrow future: object): void
```

## `start`

Explicitly launches a lazy future without awaiting it (fire-and-forget). Futures only run when awaited, passed to `all`/`any`/`race`, or started here; a never-started future never executes. Takes the future as `object` (futures are heap refs) like `cancel`.

```dream
public static extern fun start(borrow future: object): void
```

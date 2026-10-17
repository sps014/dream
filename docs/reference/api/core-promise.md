# Promise

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Promise`

Promise utilities for async tasks.

```dream
public static class Promise
```

## `all`

Waits for all promises in the array to resolve.

```dream
public static async fun all<T>(
    borrow promises: Future<T>[],
    token: Option<CancellationToken> = Option.None
): T[]
```

## `any`

Waits for any promise in the array to resolve.

```dream
public static async fun any<T>(
    borrow promises: Future<T>[],
    token: Option<CancellationToken> = Option.None
): T
```

## `race`

Returns the first promise to resolve or reject.

```dream
public static async fun race<T>(
    borrow promises: Future<T>[],
    token: Option<CancellationToken> = Option.None
): T
```

## `cancel`

Requests cancellation of a future and of its awaited work when no other owner needs it. Cancellation of an in-flight network request is best effort; use `CancellationToken` for cooperative application cancellation.

```dream
public static extern fun cancel(borrow future: object): void
```

## `start`

Explicitly launches a lazy future without awaiting it (fire-and-forget). Futures only run when awaited, passed to `all`/`any`/`race`, or started here; a never-started future never executes. Takes the future as `object` (futures are heap refs) like `cancel`.

```dream
public static extern fun start(borrow future: object): void
```

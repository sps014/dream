# Cancellation Token

**Import:** `import system.core;`

## `CancellationToken`

Read-only view of a `CancellationSource` for cooperative checks at await boundaries. `@shared` (closed-graph over `@shared` `CancellationSource`) so tokens can cross into workers.

```dream
shared public class CancellationToken
```

## `current`

Returns the cancellation token inherited from the current structured task.

```dream
public static get current(): Option<CancellationToken>
```

## `resolve`

Explicit tokens override the current structured task; None inherits its token.

```dream
public static fun resolve(token: Option<CancellationToken>): Option<CancellationToken>
```

## `is_cancelled`

True when the owning source has been cancelled.

```dream
public get is_cancelled(): bool
```

## `wait_cancelled`

Suspends until cancellation and returns true. Completion or releasing a cancelled future detaches its subscription.

```dream
public fun wait_cancelled(): Future<bool>
```

## `wait`

Awaits a future or cancellation using scheduler notifications. Cancellation cancels the supplied future; already performed side effects are preserved.

```dream
public fun wait<T>(future: Future<T>): Future<Result<T, CancelledError>>
```

## `wait_void`

Awaits a void future or cancellation, returning true when the operation completes.

```dream
public fun wait_void(future: Future<void>): Future<Result<Unit, CancelledError>>
```

## `throw_if_cancelled`

Panics with `CancelledError` when already cancelled.

```dream
public fun throw_if_cancelled(): void
```

## `check`

`Ok(Unit())` when still running; `Err(CancelledError)` when cancelled.

```dream
public fun check(): Result<Unit, CancelledError>
```

## `is_cancelled_opt`

True when `token` is `Some` and already cancelled.

```dream
public static fun is_cancelled_opt(token: Option<CancellationToken>): bool
```

## `throw_if_opt`

Panics when `token` is `Some` and already cancelled.

```dream
public static fun throw_if_opt(token: Option<CancellationToken>): void
```

## `rejected`

Returns err when the explicit or inherited token is cancelled; otherwise Ok(Unit()).

```dream
public static fun rejected<E>(token: Option<CancellationToken>, err: E): Result<Unit, E>
```

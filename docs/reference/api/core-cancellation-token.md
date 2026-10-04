# Cancellation Token

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `is_cancelled`

True when the owning source has been cancelled.

```dream
public get is_cancelled(): bool
```

## `throw_if_cancelled`

Panics with `CancelledError` when already cancelled.

```dream
public fun throw_if_cancelled(): void
```

## `check`

`Ok(true)` when still running; `Err(CancelledError)` when cancelled.

```dream
public fun check(): Result<bool, CancelledError>
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

`Err(err)` when `token` is cancelled; otherwise `Ok(true)`.

```dream
public static fun rejected<E>(token: Option<CancellationToken>, err: E): Result<bool, E>
```

# Semaphore

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `constructor`

Creates a semaphore with `initial` available permits.

```dream
public constructor(initial: int)
```

## `host_acquire`

Acquires the supplied synchronization object. Prefer its instance `acquire()` method.

```dream
public static extern fun host_acquire(borrow target: Semaphore): void
```

## `host_release`

Releases the supplied synchronization object. Prefer its instance `release()` method.

```dream
public static extern fun host_release(borrow target: Semaphore): void
```

## `host_try_acquire`

Attempts acquisition once and returns whether it succeeded.

```dream
public static extern fun host_try_acquire(borrow target: Semaphore): bool
```

## `host_try_acquire_for`

Attempts acquisition for up to `timeout_ms` milliseconds; non-positive limits perform a single attempt.

```dream
public static extern fun host_try_acquire_for(borrow target: Semaphore, timeout_ms: int): bool
```

## `acquire`

Blocks (busy-waiting) until a permit is available, then takes one.

```dream
public fun acquire(): void
```

## `try_acquire`

One-shot acquire: returns `true` if a permit was taken, `false` if none were available.

```dream
public fun try_acquire(): bool
```

## `try_acquire_for`

Waits up to `timeout_ms` milliseconds for a permit. `timeout_ms <= 0` is a single try.

```dream
public fun try_acquire_for(timeout_ms: int): bool
```

## `release`

Returns one permit.

```dream
public fun release(): void
```

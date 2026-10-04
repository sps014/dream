# Semaphore

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `constructor`

Creates a semaphore with `initial` available permits.

```dream
public constructor(initial: int)
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

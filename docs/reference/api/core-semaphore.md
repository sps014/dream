# Semaphore

**Import:** `import system.core;`

## `Semaphore`

Counting semaphore for limiting concurrent access to a shared resource.

```dream
shared public class Semaphore
```

## `constructor`

Creates a semaphore with `initial` available permits.

```dream
public constructor(initial: int)
```

## `host_acquire`

```dream
public static extern fun host_acquire(borrow target: Semaphore): void
```

## `host_release`

```dream
public static extern fun host_release(borrow target: Semaphore): void
```

## `host_try_acquire`

```dream
public static extern fun host_try_acquire(borrow target: Semaphore): bool
```

## `host_try_acquire_for`

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

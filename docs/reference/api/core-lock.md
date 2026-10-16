# Lock

**Import:** `import system.core;`

## `Lock`

Reentrant mutual-exclusion lock for `@shared` critical sections.

```dream
shared public class Lock
```

## `host_acquire`

Waits until the synchronization object can be acquired.

```dream
public static extern fun host_acquire(borrow target: Lock): void
```

## `host_release`

Releases the synchronization object after acquisition.

```dream
public static extern fun host_release(borrow target: Lock): void
```

## `host_try_acquire`

Attempts acquisition without waiting; returns whether it succeeded.

```dream
public static extern fun host_try_acquire(borrow target: Lock): bool
```

## `host_try_acquire_for`

Attempts acquisition for up to `timeout_ms` milliseconds.

```dream
public static extern fun host_try_acquire_for(borrow target: Lock, timeout_ms: int): bool
```

## `acquire`

Reentrant: the same thread may call `acquire` again before releasing (matching `lock (obj) { ... }`'s own semantics), and must call `release` the same number of times to fully release it.

```dream
public fun acquire(): void
```

## `try_acquire`

One-shot acquire: returns `true` on success (including a reentrant bump), `false` if another thread currently holds the lock.

```dream
public fun try_acquire(): bool
```

## `try_acquire_for`

Waits up to `timeout_ms` milliseconds to acquire. `timeout_ms <= 0` is a single try. Best-effort under contention: spurious wakes do not shrink the remaining wait budget.

```dream
public fun try_acquire_for(timeout_ms: int): bool
```

## `release`

Releasing without ownership is a runtime panic, including release of an unheld lock.

```dream
public fun release(): void
```

# Lock

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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

---
title: Lock and Semaphore
summary: Coordinate access to shared state between tasks.
icon: material/lock-outline
api: reference/api/system-core.md
---

# Lock and Semaphore

A lock protects changes to a shared value. A semaphore limits how many tasks may enter a section at once.

```dream
import system;

fun main() {
    let lock = Lock();
    lock.acquire();
    // …shared work…
    lock.release();
}
```

## `Lock`

| Call | Meaning |
| --- | --- |
| `acquire()` / `release()` | take / drop the lock (blocks) |
| `try_acquire()` | `true` if taken now |
| `try_acquire_for(ms)` | wait up to `ms` milliseconds |

Locks are reentrant: each successful acquisition must be matched by a release on the same
thread. Releasing an unheld lock, or releasing from another thread, panics with
`panic: lock release requires the owning thread`.

A timed `try_acquire_for` keeps one deadline for the whole wait. Destroying a lock starts the next lock fresh, including if its storage is reused.

## `Semaphore`

`Semaphore(initial)` then `acquire()` / `release()` — a counting permit.

```dream
let gate = Semaphore(2);
gate.acquire();
// …at most two holders…
gate.release();
```

## Cancellation


```dream
let src = CancellationSource();
let tok = src.token;
src.cancel();
System.println(tok.is_cancelled); // true
```

Cross-thread locking only works when workers share memory (native, or a browser page that is allowed to share memory — see [Tasks](../../language/concurrency/tasks.md)).

# Lock and Semaphore

Use locks and semaphores to coordinate tasks that share work or data. A lock protects changes to a shared value. A semaphore limits how many tasks may enter a section at once.

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

Native waiters sleep on a condition belonging to that object, and timed acquisition keeps one
deadline across spurious wakes. Native lock-registry entries are removed when their objects are
destroyed, so a reused heap address starts with a fresh lock. WASM uses the object's lock word.

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
System.println(tok.is_cancelled);   // true
```

Cross-thread locking only works when workers share memory (native, or a browser page that is allowed to share memory — see [Tasks](../language/tasks.md)).

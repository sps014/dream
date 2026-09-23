# Lock and Semaphore

Coordinate [Task](../language/tasks.md) threads that share objects. Both types are `shared class`, so a worker body can capture them.

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

## `Semaphore`

`Semaphore(initial)` then `acquire()` / `release()` — a counting permit.

```dream
let gate = Semaphore(2);
gate.acquire();
// …at most two holders…
gate.release();
```

## Cancellation

`CancellationSource` has `cancel()`, `is_cancelled`, and `token`. Pass `token: Some(tok)` as the last argument of stdlib async APIs, or attach it with [`HttpClient.with_cancellation`](http.md). `CancellationToken.is_cancelled` is the read-only side.

```dream
let src = CancellationSource();
let tok = src.token;
src.cancel();
System.println(tok.is_cancelled);   // true
```

Cross-thread locking only works when workers share memory (native, or a browser page that is allowed to share memory — see [Tasks](../language/tasks.md)).

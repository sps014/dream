# Cancel task work

Give long-running work a way to stop, and decide where it checks for cancellation.

[Back to overview](tasks.md)

## Cancellation

Two layers:

**Cooperative (preferred):** capture a `shared` `CancellationToken` and poll it; the owner calls `CancellationSource.cancel()`.

```dream
let src = CancellationSource();
let tok = src.token;
let w = Task.spawn(() => {
    while !tok.is_cancelled { /* work */ }
    return 0;
});
src.cancel();
let _ = w.await;
```

**Hard abort:** `Promise.cancel(w)` (or dropping the Future) stops the task immediately. The browser terminates the worker; native `dream run` detaches the OS thread if the body is still running. Hard abort does **not** unwind the task — its pending releases and `del` destructors never run, and its private memory may be abandoned — prefer the token when `shared` state must stay consistent.

A task that has already finished its body is joined and its env is released (`Debug.live_objects` stays flat across repeated `spawn`).


## Async task bodies

A task body may `await` via `spawn_async` (named `async fun` or `async` lambda):

```dream
async fun main(): void {
    let n = 6;
    let squarer = Task.spawn_async(async () => {
        Time.sleep(1).await;
        return n * n;
    });
    System.println((squarer.await).to_string()); // 36
}
```

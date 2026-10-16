---
title: Task cancellation
summary: Give long-running work a way to stop cleanly.
icon: material/cancel
status: stable
targets: [native, web, node]
---

# Cancel task work

Give long-running work a way to stop, and decide where it checks for cancellation.

!!! glance "At a glance"

    ```dream
    let source = CancellationSource();
    let token = source.token;
    source.cancel();
    ```

    - Pass a cancellation token to long-running work and check it regularly.
    - Cooperative cancellation lets work stop at a safe point.

[Back to overview](tasks.md)

## Cancellation

Two layers:

**Cooperative (preferred):** capture a `shared` `CancellationToken` and poll it; the owner calls `CancellationSource.cancel()`.

```dream
let src = CancellationSource();
let tok = src.token;
let w = Task.spawn(
    () => {
        while !tok.is_cancelled { /* work */
        }
        return 0;
    }
);
src.cancel();
let _ = w.await;
```

**Hard abort:** `Promise.cancel(w)` (or dropping the Future) stops the task immediately. The browser terminates the worker; native `dream run` detaches the OS thread if the body is still running. Hard abort does **not** unwind the task — its pending releases and `del` destructors never run, and its private memory may be abandoned — prefer the token when `shared` state must stay consistent.

A task that has already finished its body is joined and its env is released (`Debug.live_objects` stays flat across repeated `spawn`).

## Ways to stop work

| Call | Kind | Effect |
|------|------|--------|
| `source.cancel()` | Cooperative | Every token from `source` reports `is_cancelled`; work stops where it checks |
| `scope.cancel()` | Cooperative | Cancels the token a [task scope](structured.md) gives its children; the scope still waits for them to finish |
| `Promise.cancel(future)` | Hard abort | Stops the future immediately, without running the task's cleanup |

## `CancellationSource` members

`CancellationSource` and `CancellationToken` are available without an import. Both are `shared` classes, so tasks can capture them.

| Member | Returns | Effect |
|--------|---------|--------|
| `CancellationSource()` | `CancellationSource` | A new source that is not cancelled |
| `.cancel()` | — | Marks the source and all of its tokens as cancelled |
| `.is_cancelled` | `bool` | `true` after `cancel()` |
| `.token` | `CancellationToken` | A read-only token that observes this source |

## `CancellationToken` members

| Member | Returns | Effect |
|--------|---------|--------|
| `.is_cancelled` | `bool` | `true` once the source is cancelled |
| `.check()` | `Result<bool, CancelledError>` | `Ok(true)` while running; `Err(CancelledError)` once cancelled |
| `.throw_if_cancelled()` | — | Panics with a cancellation message if cancelled |
| `.wait_cancelled()` | `Future<bool>` | Completes with `true` when cancellation happens |
| `.wait(future)` | `Future<Result<T, CancelledError>>` | Awaits `future`, or returns `Err` and cancels `future` if the token is cancelled first |
| `.wait_void(future)` | `Future<Result<bool, CancelledError>>` | Same as `wait`, for a `Future<void>` |
| `CancellationToken.current` | `Option<CancellationToken>` | Static; the token inherited from the enclosing task scope, if any |

`Task.scope`, `Task.spawn`, `Task.spawn_async`, `Task.map`, `Task.for_each`, and the `TaskPool` dispatch and map methods all take an optional `token:` argument. When you leave it out, scopes, maps, and pool methods use the token inherited from the enclosing scope.


## Async task bodies

A task body may `await` via `spawn_async` (named `async fun` or `async` lambda):

```dream
async fun main(): void {
    let n = 6;
    let squarer = Task.spawn_async(
        async () => {
            Time.sleep(1).await;
            return n * n;
        }
    );
    System.println((squarer.await).to_string()); // 36
}
```

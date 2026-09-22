# Async / Await

**Packages:** `Promise` / `Future` are bootstrap (`system.core`). `Time.sleep` needs `import system;`. HTTP examples need `import system.net;`.

Dream has cooperative concurrency with `async`/`await`.
The model is **lazy**: calling an `async fun` builds a `Future<T>` but does *not* run it.
The body runs when the future is first **awaited**, passed to a combinator (`Promise.all` / `any` / `race`), or launched with `Promise.start`.

A future that is never started never runs, and a cancelled-before-start future is simply discarded.

## Declaring and awaiting

Prefix a function with `async`.
Its declared return type `T` becomes `Future<T>` at the call site.
`e.await` suspends the current task until `e`'s future resolves, then yields its value:

```dream
import system;

async fun fetchData(): string {
    Time.sleep(100).await;   // suspends this task; other work can keep running
    return "data";
}

async fun main(): void {
    let x = fetchData().await;   // x : string
    System.println(x);
}
```

`f().await` is just the call composed with `.await`: `f()` produces a `Future<T>`, and `.await` suspends on it to get `T`.
The only rule is that `.await` outside an `async` function is an error.

### Where `.await` is allowed

`.await` may appear in any expression or statement position inside an `async` function — including conditionally evaluated ones:

```dream
let x = e.await;                  // bind the result
let y = f().await + 1;            // in an operand
process(a().await, b().await);    // several awaits in call arguments

if retry { data = fetch(url).await; }           // in a branch
while i < n { sum += g(i).await; i += 1; }      // suspends each iteration
let y = cond ? a().await : b().await;           // in a ternary arm
let z = flag && ready().await;                  // right side of && / || / ??
```

### Awaiting a `Result`

Because `.await` is a postfix step, a trailing `?` applies to the awaited value rather than to the future, so propagating an error out of an async call needs no parentheses:

```dream
async fun load(url: string): Result<string, string> {
    let body = fetch(url).await?;   // await the future, then propagate its Err
    return Result.Ok(body.trim());
}
```

Chaining works the same way — `.await` is an ordinary link in a postfix chain, so `parse(src).await.value` and `rows().await[0]` are both fine.

## Running work concurrently

`.await` starts the future it awaits, so a plain `let x = work().await;` runs alone.
To run several futures concurrently, hand them to a combinator (which starts every member) or launch them explicitly:

```dream
import system;

async fun work(id: int): int {
    Time.sleep(50).await;
    return id * id;
}

async fun main(): void {
    let a = work(2);                         // constructed, not yet running
    let b = work(3);                         // constructed, not yet running
    let results = Promise.all([a, b]).await; // starts both -> they run concurrently -> [4, 9]
    System.println(results[0] + ", " + results[1]);
}
```

### Fire-and-forget (`Promise.start`)

`Promise.start(future)` schedules a future without awaiting it.
The runtime keeps that future until it settles, so you do not need to keep a local after `start`:

```dream
let f = logLater();     // nothing runs yet
Promise.start(f);       // launches it; result is discarded
Time.sleep(10).await;   // give it a chance to run
```

A future that is neither started nor awaited never executes — dropping it just releases its captured state.
`Promise.cancel(f)` before the first start means it never will.

### Combinators (`Promise`)

Static methods on the built-in `Promise` class, over `Future<T>[]`:

| Method | Signature | Resolves when |
| --- | --- | --- |
| `Promise.all` | `Promise.all(xs: Future<T>[]): Future<T[]>` | every future has resolved (results in order) |
| `Promise.any` | `Promise.any(xs: Future<T>[]): Future<T>` | the first future resolves |
| `Promise.race` | `Promise.race(xs: Future<T>[]): Future<T>` | the first future settles |

```dream
let first = Promise.any([work(10), work(20)]).await;
```

`Time.sleep(ms: int, token: Option<CancellationToken> = None): Future<void>` is an awaitable timer.
On the native host it uses a virtual clock; in the browser it uses `setTimeout`.
With a token, cancellation is noticed during the sleep.

It composes with the combinators like any other future.

## Cancellation

Bootstrap types `CancellationSource` / `CancellationToken` / `CancelledError` support cooperative cancellation:

```dream
let src = CancellationSource();
let tok = src.token;
src.cancel();
System.println(tok.check().is_err()); // true → ECANCELLED
```

Public stdlib async APIs take a trailing `token: Option<CancellationToken> = None` (omitted at existing call sites).
`Result` methods return `Err` with machine code `ECANCELLED`; `void` / non-`Result` APIs panic via `throw_if_cancelled`.
`Promise.cancel(future)` marks a future cancelled; pending sleeps are dropped when the token is cancelled.

Cancelling a not-yet-started future means it never runs.
Native in-flight host I/O remains best-effort (`HttpClient.with_cancellation` still sets a client-wide default used when the per-call token is omitted).

### Native deferred hosts (`@async_host`)

On native, an `extern async fun` host blocks the whole run loop while it runs.
Declaring it `@async_host` opts that import into true async: on the native host, the function runs off the main task and completes the future when it finishes — so timers and other tasks keep interleaving while the host op is in flight.
`HttpClient` request methods use this on native; wasm32 bridges are always deferred.

## Async methods

Instance and `static` class methods can be `async`, so a type can own its asynchronous behavior.
The call types as `Future<T>` just like a free async call:

```dream
import system;
import system.net;

class Downloader {
    url: string;
    async fun fetch(): string {
        let body = HttpClient().text(this.url).await;
        return body;
    }
}

async fun main(): void {
    let d = Downloader("https://example.com");
    let body = d.fetch().await;   // d.fetch() : Future<string>
    System.println(body);
}
```

Async methods work on **generic** classes too: each concrete type gets its own async method.

### Async lambdas and `fun(...): Future<T>` values

An `async (params) => …` arrow lambda is typed as `fun(...): Future<T>` — see [Functions](functions.md#async-lambdas).
Calling the boxed value returns a `Future` just like calling a named `async fun`; `await` unwraps it.
Named async functions used as first-class values (`let f: fun(int): Future<int> = delayed;`) use the same shape.

## Awaiting JavaScript promises

An `extern async fun` bridges to a host function that returns a Promise.
Like every other future, the bridge is lazy: calling it does *not* invoke the host yet — the call happens when the returned future is first awaited, started, or passed to a combinator.
Dream source never sees the Promise itself:

```dream
@js("api", "getUser")
extern async fun getUser(id: int): string;

async fun main(): void {
    let name = getUser(42).await;
    System.println("user = " + name);
}
```

```js
import { run } from "./dream.js";

await run("user.wasm", {
  imports: {
    getUser: (id) => fetch(`/api/user/${id}`).then((r) => r.text()),
  },
});
```

A complete example: [`sample/interop/async_fetch.dream`](https://github.com/sps014/dream/blob/main/sample/interop/async_fetch.dream).

## Limitations

- No `.then()` / callback chaining — use `async` / `await`.
- Tasks interleave at `await` points on one thread. For real parallelism, see [Tasks](tasks.md).

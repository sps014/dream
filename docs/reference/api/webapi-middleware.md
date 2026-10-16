# Middleware

**Import:** `import system.webapi;`

See the [Web APIs guide](../../web-server/index.md) for runnable examples, lifecycle, and error behavior.

## `RequestContext`

Per-request bag shared by middleware and `@dep` functions.

```dream
public class RequestContext
```

## `incoming`

Request received from the client.

```dream
public incoming: HttpIncoming
```

## `app_state`

Application values shared by requests.

```dream
public app_state: Map<string, string>
```

## `state`

Values stored for this request by handlers and middleware.

```dream
public state: Map<string, string>
```

## `cancellation_token`

Cancellation signal for the current request, when available.

```dream
public cancellation_token: Option<CancellationToken>
```

## `constructor`

Creates a `RequestContext` with `incoming`.

```dream
public constructor(incoming: HttpIncoming)
```

## `add_cleanup`

Registers request cleanup, run once in reverse acquisition order outside request cancellation.

```dream
public fun add_cleanup(cleanup: fun(CancellationToken): Future<void>): void
```

## `set`

Replaces the element at `index`; an out-of-range index panics.

```dream
public fun set(key: string, value: string): void
```

## `get`

Returns the element at `index`; an out-of-range index panics.

```dream
public fun get(key: string): Option<string>
```

## `Next`

Continues the middleware onion toward the route handler.

```dream
public class Next
```

## `constructor`

Creates a `Next` with `step`.

```dream
public constructor(step: fun(): Future<HttpOutgoing>)
```

## `call`

Runs the next step in the middleware pipeline.

```dream
public fun call(): Future<HttpOutgoing>
```

## `Middleware`

One onion layer: `invoke` may short-circuit or `next.call().await`.

```dream
public class Middleware
```

## `constructor`

Creates a `Middleware` with `handler`.

```dream
public constructor(handler: fun(RequestContext, Next): Future<HttpOutgoing>)
```

## `invoke`

Runs this middleware for `ctx`, using `next` to continue the request pipeline.

```dream
public fun invoke(borrow ctx: RequestContext, borrow next: Next): Future<HttpOutgoing>
```

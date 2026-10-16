# Middleware

**Import:** `import system.webapi;`

See the [Web APIs guide](../../web-apis/index.md) for runnable examples, lifecycle, and error behavior.

## `public class RequestContext`

Per-request bag shared by middleware and `@dep` functions.

```dream
public class RequestContext
```

## `public incoming`

```dream
public incoming: HttpIncoming
```

## `public app_state`

```dream
public app_state: Map<string, string>
```

## `public state`

```dream
public state: Map<string, string>
```

## `public cancellation_token`

```dream
public cancellation_token: Option<CancellationToken>
```

## `public constructor`

```dream
public constructor(incoming: HttpIncoming)
```

## `public fun add_cleanup`

Registers request cleanup, run once in reverse acquisition order outside request cancellation.

```dream
public fun add_cleanup(cleanup: fun(CancellationToken): Future<void>): void
```

## `public fun set`

```dream
public fun set(key: string, value: string): void
```

## `public fun get`

```dream
public fun get(key: string): Option<string>
```

## `public class Next`

Continues the middleware onion toward the route handler.

```dream
public class Next
```

## `public constructor`

```dream
public constructor(step: fun(): Future<HttpOutgoing>)
```

## `public fun call`

```dream
public fun call(): Future<HttpOutgoing>
```

## `public class Middleware`

One onion layer: `invoke` may short-circuit or `next.call().await`.

```dream
public class Middleware
```

## `public constructor`

```dream
public constructor(handler: fun(RequestContext, Next): Future<HttpOutgoing>)
```

## `public fun invoke`

```dream
public fun invoke(borrow ctx: RequestContext, borrow next: Next): Future<HttpOutgoing>
```

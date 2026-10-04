# RequestContext, Next, Middleware

**Import:** `import system.webapi;`

Read the [usage guide](../stdlib/index.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class RequestContext`

Per-request bag shared by middleware and `@dep` functions.

```dream
public class RequestContext
```

## `incoming: HttpIncoming`

```dream
public incoming: HttpIncoming
```

## `state: Map<string, string>`

```dream
public state: Map<string, string>
```

## `cancellation_token: Option<CancellationToken>`

```dream
public cancellation_token: Option<CancellationToken>
```

## `constructor`

```dream
public constructor(incoming: HttpIncoming)
```

## `set`

```dream
public fun set(key: string, value: string): void
```

## `get`

```dream
public fun get(key: string): Option<string>
```

## `class Next`

Continues the middleware onion toward the route handler.

```dream
public class Next
```

## `constructor`

```dream
public constructor(step: fun(): Future<HttpOutgoing>)
```

## `call`

```dream
public async fun call(): HttpOutgoing
```

## `class Middleware`

One onion layer: `invoke` may short-circuit or `next.call().await`.

```dream
public class Middleware
```

## `constructor`

```dream
public constructor(handler: fun(RequestContext, Next): Future<HttpOutgoing>)
```

## `invoke`

```dream
public async fun invoke(borrow ctx: RequestContext, borrow next: Next): HttpOutgoing
```

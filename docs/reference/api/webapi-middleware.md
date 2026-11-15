# Middleware

**Import:** `import system.webapi;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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

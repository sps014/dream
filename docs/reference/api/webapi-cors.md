# Cors

**Import:** `import system.webapi;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Cors`

Middleware applying cross-origin access rules to requests and preflight responses.

```dream
public class Cors
```

## `invoke`

Runs this middleware for `ctx`, using `next` to continue the request pipeline.

```dream
public fun invoke(borrow ctx: RequestContext, borrow next: Next): Future<HttpOutgoing>
```

## `middleware`

Returns middleware that applies `opts` to every request: it answers preflight `OPTIONS` requests and adds the allowed-origin headers to responses. Register it with `app.use(Cors.middleware(CorsOptions()))`.

```dream
public static fun middleware(opts: CorsOptions): Middleware
```

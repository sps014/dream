# Cors

**Import:** `import system.webapi;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Cors`

Middleware applying cross-origin access rules to requests and preflight responses.

```dream
public class Cors
```

## `constructor`

Creates a `Cors` with `opts`.

```dream
public constructor(opts: CorsOptions)
```

## `invoke`

Runs this middleware for `ctx`, using `next` to continue the request pipeline.

```dream
public fun invoke(borrow ctx: RequestContext, borrow next: Next): Future<HttpOutgoing>
```

## `CORS`

`app.use(CORS(CorsOptions())).unwrap()` — same role as FastAPI `add_middleware(CORSMiddleware, ...)`.

```dream
public fun CORS(opts: CorsOptions): Middleware
```

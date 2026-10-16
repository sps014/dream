# Cors

**Import:** `import system.webapi;`

See the [Web APIs guide](../../web-server/index.md) for runnable examples, lifecycle, and error behavior.

## `CorsOptions`

FastAPI / Starlette `CORSMiddleware` options. Origins, methods, and headers are comma-separated (`"*"` means any). `allow_credentials` cannot be combined with origin `*` (the request Origin is echoed instead, matching Starlette).

```dream
public class CorsOptions
```

## `allow_origins`

Allowed request origins, or `*` for any origin.

```dream
public allow_origins: string
```

## `allow_methods`

Allowed HTTP methods, or `*` to allow all.

```dream
public allow_methods: string
```

## `allow_headers`

Allowed request headers, or `*` to allow all.

```dream
public allow_headers: string
```

## `expose_headers`

Response headers that browsers may expose to cross-origin callers.

```dream
public expose_headers: string
```

## `allow_credentials`

Whether cross-origin requests may include credentials.

```dream
public allow_credentials: bool
```

## `max_age`

How long browsers may cache preflight results, in seconds.

```dream
public max_age: int
```

## `constructor`

Creates a `CorsOptions` with `allow_origins`, `allow_methods`, `allow_headers`, `expose_headers`, `allow_credentials`, `max_age`.

```dream
public constructor(
    allow_origins: string = "*",
    allow_methods: string = "*",
    allow_headers: string = "*",
    expose_headers: string = "",
    allow_credentials: bool = false,
    max_age: int = 600
)
```

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

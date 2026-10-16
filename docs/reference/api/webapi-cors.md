# Cors

**Import:** `import system.webapi;`

See the [Web APIs guide](../../web-apis/index.md) for runnable examples, lifecycle, and error behavior.

## `public class CorsOptions`

FastAPI / Starlette `CORSMiddleware` options. Origins, methods, and headers are comma-separated (`"*"` means any). `allow_credentials` cannot be combined with origin `*` (the request Origin is echoed instead, matching Starlette).

```dream
public class CorsOptions
```

## `public allow_origins`

```dream
public allow_origins: string
```

## `public allow_methods`

```dream
public allow_methods: string
```

## `public allow_headers`

```dream
public allow_headers: string
```

## `public expose_headers`

```dream
public expose_headers: string
```

## `public allow_credentials`

```dream
public allow_credentials: bool
```

## `public max_age`

```dream
public max_age: int
```

## `public constructor`

```dream
public constructor( allow_origins: string = "*", allow_methods: string = "*", allow_headers: string = "*", expose_headers: string = "", allow_credentials: bool = false, max_age: int = 600 )
```

## `public class Cors`

```dream
public class Cors
```

## `public constructor`

```dream
public constructor(opts: CorsOptions)
```

## `public fun invoke`

```dream
public fun invoke(borrow ctx: RequestContext, borrow next: Next): Future<HttpOutgoing>
```

## `public fun CORS`

`app.use(CORS(CorsOptions())).unwrap()` — same role as FastAPI `add_middleware(CORSMiddleware, ...)`.

```dream
public fun CORS(opts: CorsOptions): Middleware
```

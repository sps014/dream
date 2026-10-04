# CorsOptions, Cors

**Import:** `import system.webapi;`

Read the [usage guide](../stdlib/index.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class CorsOptions`

FastAPI / Starlette `CORSMiddleware` options. Origins, methods, and headers are comma-separated (`"*"` means any). `allow_credentials` cannot be combined with origin `*` (the request Origin is echoed instead, matching Starlette).

```dream
public class CorsOptions
```

## `allow_origins: string`

```dream
public allow_origins: string
```

## `allow_methods: string`

```dream
public allow_methods: string
```

## `allow_headers: string`

```dream
public allow_headers: string
```

## `expose_headers: string`

```dream
public expose_headers: string
```

## `allow_credentials: bool`

```dream
public allow_credentials: bool
```

## `max_age: int`

```dream
public max_age: int
```

## `constructor`

```dream
public constructor( allow_origins: string = "*", allow_methods: string = "*", allow_headers: string = "*", expose_headers: string = "", allow_credentials: bool = false, max_age: int = 600 )
```

## `class Cors`

```dream
public class Cors
```

## `constructor`

```dream
public constructor(opts: CorsOptions)
```

## `invoke`

```dream
public async fun invoke(borrow ctx: RequestContext, borrow next: Next): HttpOutgoing
```

## `CORS`

`WebApp.use(CORS(CorsOptions()))` — same role as FastAPI `add_middleware(CORSMiddleware, ...)`.

```dream
public fun CORS(opts: CorsOptions): Middleware
```

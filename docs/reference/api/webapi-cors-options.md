# CorsOptions

**Import:** `import system.webapi;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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

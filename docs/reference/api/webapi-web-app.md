# WebAppOptions, WebApp

**Import:** `import system.webapi;`

Read the [usage guide](../stdlib/index.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class WebAppOptions`

FastAPI-style listen options. Empty `docs_url` / `openapi_url` disables that route.

```dream
public class WebAppOptions
```

## `title: string`

```dream
public title: string
```

## `version: string`

```dream
public version: string
```

## `docs_url: string`

```dream
public docs_url: string
```

## `openapi_url: string`

```dream
public openapi_url: string
```

## `redoc_url: string`

```dream
public redoc_url: string
```

## `tls_cert_path: string`

```dream
public tls_cert_path: string
```

## `tls_key_path: string`

```dream
public tls_key_path: string
```

## `constructor`

```dream
public constructor( title: string = "Dream API", version: string = "0.1.0", docs_url: string = "/docs", openapi_url: string = "/openapi.json", redoc_url: string = "/redoc", tls_cert_path: string = "", tls_key_path: string = "" )
```

## `class WebApp`

```dream
public class WebApp
```

## `use`

```dream
public static fun use(mw: Middleware): void
```

## `listen`

```dream
public static async fun listen(host: string, port: int, token: Option<CancellationToken> = Option.None): int
```

## `listen_with`

```dream
public static async fun listen_with(host: string, port: int, options: WebAppOptions, token: Option<CancellationToken> = Option.None): int
```

## `run`

```dream
public static async fun run(host: string, port: int, token: Option<CancellationToken> = Option.None): void
```

## `run_with`

```dream
public static async fun run_with(host: string, port: int, options: WebAppOptions, token: Option<CancellationToken> = Option.None): void
```

## `shutdown`

```dream
public static fun shutdown(): void
```

## `wait`

```dream
public static async fun wait(token: Option<CancellationToken> = Option.None): void
```

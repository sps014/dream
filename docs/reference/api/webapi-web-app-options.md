# Web App Options

**Import:** `import system.webapi;`

For behavior and native target requirements, see [Native Web APIs](../stdlib/webapi.md).

## `public class WebAppOptions`

FastAPI-style listen options. Empty `docs_url` / `openapi_url` disables that route.

```dream
public class WebAppOptions
```

## `public title: string`

```dream
public title: string
```

## `public version: string`

```dream
public version: string
```

## `public docs_url: string`

```dream
public docs_url: string
```

## `public openapi_url: string`

```dream
public openapi_url: string
```

## `public redoc_url: string`

```dream
public redoc_url: string
```

## `public tls_cert_path: string`

```dream
public tls_cert_path: string
```

## `public tls_key_path: string`

```dream
public tls_key_path: string
```

## `public constructor`

```dream
public constructor( title: string = "Dream API", version: string = "0.1.0", docs_url: string = "/docs", openapi_url: string = "/openapi.json", redoc_url: string = "/redoc", tls_cert_path: string = "", tls_key_path: string = "" )
```

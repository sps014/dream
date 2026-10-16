# Web App Options

**Import:** `import system.webapi;`

See the [Web APIs guide](../../web-apis/index.md) for runnable examples, lifecycle, and error behavior.

## `public class WebAppOptions`

API metadata, documentation endpoints and native TLS settings. Empty URLs disable endpoints.

```dream
public class WebAppOptions
```

## `public title`

```dream
public title: string
```

## `public version`

```dream
public version: string
```

## `public docs_url`

```dream
public docs_url: string
```

## `public openapi_url`

```dream
public openapi_url: string
```

## `public redoc_url`

```dream
public redoc_url: string
```

## `public tls_cert_path`

```dream
public tls_cert_path: string
```

## `public tls_key_path`

```dream
public tls_key_path: string
```

## `public summary`

```dream
public summary: string
```

## `public description`

```dream
public description: string
```

## `public terms_of_service`

```dream
public terms_of_service: string
```

## `public swagger`

```dream
public swagger: SwaggerOptions
```

## `public constructor`

```dream
public constructor( title: string = "Dream API", version: string = "0.1.0", docs_url: string = "/docs", openapi_url: string = "/openapi.json", redoc_url: string = "/redoc", tls_cert_path: string = "", tls_key_path: string = "", summary: string = "", description: string = "", terms_of_service: string = "" )
```

# Web App Options

**Import:** `import system.webapi;`

See the [Web APIs guide](../../web-server/index.md) for runnable examples, lifecycle, and error behavior.

## `WebAppOptions`

API metadata, documentation endpoints and native TLS settings. Empty URLs disable endpoints.

```dream
public class WebAppOptions
```

## `title`

Title displayed in the API documentation.

```dream
public title: string
```

## `version`

Version identifying this API or snapshot format.

```dream
public version: string
```

## `docs_url`

Path serving Swagger UI; an empty path disables it.

```dream
public docs_url: string
```

## `openapi_url`

Path serving the OpenAPI document.

```dream
public openapi_url: string
```

## `redoc_url`

Path serving ReDoc; an empty path disables it.

```dream
public redoc_url: string
```

## `tls_cert_path`

Path to the server certificate; leave empty to serve plain HTTP.

```dream
public tls_cert_path: string
```

## `tls_key_path`

Path to the private key for the server certificate.

```dream
public tls_key_path: string
```

## `summary`

Short summary displayed in the API documentation.

```dream
public summary: string
```

## `description`

Description displayed in the API documentation.

```dream
public description: string
```

## `terms_of_service`

URL describing the API's terms of service.

```dream
public terms_of_service: string
```

## `swagger`

Options controlling the interactive API documentation.

```dream
public swagger: SwaggerOptions
```

## `constructor`

Creates a `WebAppOptions` with `title`, `version`, `docs_url`, `openapi_url`, `redoc_url`, `tls_cert_path`, `tls_key_path`, `summary`, `description`, `terms_of_service`.

```dream
public constructor(
    title: string = "Dream API",
    version: string = "0.1.0",
    docs_url: string = "/docs",
    openapi_url: string = "/openapi.json",
    redoc_url: string = "/redoc",
    tls_cert_path: string = "",
    tls_key_path: string = "",
    summary: string = "",
    description: string = "",
    terms_of_service: string = ""
)
```

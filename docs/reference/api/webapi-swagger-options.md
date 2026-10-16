# Swagger Options

**Import:** `import system.webapi;`

See the [Web APIs guide](../../web-apis/index.md) for runnable examples, lifecycle, and error behavior.

## `SwaggerOptions`

Swagger UI assets and browser behavior. Asset URLs may point to self-hosted files.

```dream
public class SwaggerOptions
```

## `js_url`

URL of the Swagger UI script.

```dream
public js_url: string
```

## `css_url`

URL of the Swagger UI stylesheet.

```dream
public css_url: string
```

## `redoc_js_url`

URL of the ReDoc script.

```dream
public redoc_js_url: string
```

## `oauth_redirect_url`

Redirect URL used after authorization from Swagger UI.

```dream
public oauth_redirect_url: string
```

## `deep_linking`

Whether Swagger UI gives operations their own URL fragments.

```dream
public deep_linking: bool
```

## `persist_authorization`

Whether Swagger UI retains authorization between reloads.

```dream
public persist_authorization: bool
```

## `try_it_out_enabled`

Whether Swagger UI enables interactive requests by default.

```dream
public try_it_out_enabled: bool
```

## `oauth_client_id`

Client identifier used by the API documentation's OAuth flow.

```dream
public oauth_client_id: string
```

## `oauth_use_pkce`

Whether the documentation's OAuth flow uses PKCE.

```dream
public oauth_use_pkce: bool
```

## `constructor`

Creates a `SwaggerOptions` with `js_url`, `https`, `css_url`, `https`, `redoc_js_url`, `https`, `oauth_redirect_url`, `deep_linking`, `persist_authorization`, `try_it_out_enabled`, `oauth_client_id`, `oauth_use_pkce`.

```dream
public constructor(
    js_url: string = "https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js",
    css_url: string = "https://unpkg.com/swagger-ui-dist@5/swagger-ui.css",
    redoc_js_url: string = "https://cdn.jsdelivr.net/npm/redoc@2/bundles/redoc.standalone.js",
    oauth_redirect_url: string = "",
    deep_linking: bool = true,
    persist_authorization: bool = false,
    try_it_out_enabled: bool = false,
    oauth_client_id: string = "",
    oauth_use_pkce: bool = true
)
```

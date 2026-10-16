# Swagger Options

**Import:** `import system.webapi;`

See the [Web APIs guide](../../web-apis/index.md) for runnable examples, lifecycle, and error behavior.

## `public class SwaggerOptions`

Swagger UI assets and browser behavior. Asset URLs may point to self-hosted files.

```dream
public class SwaggerOptions
```

## `public js_url`

```dream
public js_url: string
```

## `public css_url`

```dream
public css_url: string
```

## `public redoc_js_url`

```dream
public redoc_js_url: string
```

## `public oauth_redirect_url`

```dream
public oauth_redirect_url: string
```

## `public deep_linking`

```dream
public deep_linking: bool
```

## `public persist_authorization`

```dream
public persist_authorization: bool
```

## `public try_it_out_enabled`

```dream
public try_it_out_enabled: bool
```

## `public oauth_client_id`

```dream
public oauth_client_id: string
```

## `public oauth_use_pkce`

```dream
public oauth_use_pkce: bool
```

## `public constructor`

```dream
public constructor( js_url: string = "https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js", css_url: string = "https://unpkg.com/swagger-ui-dist@5/swagger-ui.css", redoc_js_url: string = "https://cdn.jsdelivr.net/npm/redoc@2/bundles/redoc.standalone.js", oauth_redirect_url: string = "", deep_linking: bool = true, persist_authorization: bool = false, try_it_out_enabled: bool = false, oauth_client_id: string = "", oauth_use_pkce: bool = true )
```

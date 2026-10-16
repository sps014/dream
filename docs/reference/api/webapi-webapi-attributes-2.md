# Webapi Attributes

**Import:** `import system.webapi;`

See the [Web APIs guide](../../web-apis/index.md) for runnable examples, lifecycle, and error behavior.

## `provider`

Dependency function to evaluate for this parameter or replace for this application.

```dream
public provider: DeclarationPath
```

## `ProducesAttribute`

Sets the content type of a yielded string or byte stream.

```dream
public struct ProducesAttribute
```

## `content_type`

Media type sent with the streamed response, such as `text/event-stream`.

```dream
public content_type: string
```

## `BodyAttribute`

Extracts the request body as text, bytes or a JSON model.

```dream
public struct BodyAttribute
```

## `PathAttribute`

Binds a path value, with an optional name override.

```dream
public struct PathAttribute
```

## `name`

Optional input name; omitted means use the parameter name.

```dream
public name: Option<string>
```

## `QueryAttribute`

Binds a query value, with an optional name override.

```dream
public struct QueryAttribute
```

## `name`

Optional input name; omitted means use the parameter name.

```dream
public name: Option<string>
```

## `HeaderAttribute`

Binds a header value, with an optional name override.

```dream
public struct HeaderAttribute
```

## `name`

Optional input name; omitted means use the parameter name.

```dream
public name: Option<string>
```

## `CookieAttribute`

Binds a cookie value, with an optional name override.

```dream
public struct CookieAttribute
```

## `name`

Optional input name; omitted means use the parameter name.

```dream
public name: Option<string>
```

## `FormAttribute`

Binds a form value, with an optional name override.

```dream
public struct FormAttribute
```

## `name`

Optional input name; omitted means use the parameter name.

```dream
public name: Option<string>
```

## `FileAttribute`

Binds a file value, with an optional name override.

```dream
public struct FileAttribute
```

## `name`

Optional input name; omitted means use the parameter name.

```dream
public name: Option<string>
```

## `SecuritySchemeAttribute`

Declares a dependency's OpenAPI authentication scheme.

```dream
public struct SecuritySchemeAttribute
```

## `id`

Identifier used to reference this security scheme in OpenAPI.

```dream
public id: string
```

## `kind`

OpenAPI scheme kind, such as `http`, `apiKey`, or `oauth2`.

```dream
public kind: string
```

## `scheme`

HTTP authentication scheme, such as `bearer`.

```dream
public scheme: string
```

## `name`

Name of the API-key header, query parameter, or cookie, when applicable.

```dream
public name: Option<string>
```

## `location`

Where an API key is supplied: `header`, `query`, or `cookie`.

```dream
public location: Option<string>
```

## `RequiresScopesAttribute`

Requires space-separated permission scopes from a verified CurrentUser dependency.

```dream
public struct RequiresScopesAttribute
```

## `scopes`

Space-separated permissions that the current user must have.

```dream
public scopes: string
```

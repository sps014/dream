# Webapi Attributes

**Import:** `import system.webapi;`

See the [Web APIs guide](../../web-apis/index.md) for runnable examples, lifecycle, and error behavior.

## `public struct GetAttribute`

Declares an HTTP GET route.

```dream
public struct GetAttribute
```

## `public struct PostAttribute`

Declares an HTTP POST route.

```dream
public struct PostAttribute
```

## `public struct PutAttribute`

Declares an HTTP PUT route.

```dream
public struct PutAttribute
```

## `public struct PatchAttribute`

Declares an HTTP PATCH route.

```dream
public struct PatchAttribute
```

## `public struct DeleteAttribute`

Declares an HTTP DELETE route.

```dream
public struct DeleteAttribute
```

## `public struct HeadAttribute`

Declares an HTTP HEAD route.

```dream
public struct HeadAttribute
```

## `public struct OptionsAttribute`

Declares an HTTP OPTIONS route.

```dream
public struct OptionsAttribute
```

## `public struct WebsocketAttribute`

Declares an HTTP WEBSOCKET route.

```dream
public struct WebsocketAttribute
```

## `public struct RouterAttribute`

Declares an explicitly mounted group of static HTTP handlers.

```dream
public struct RouterAttribute
```

## `public struct UseAttribute`

Applies a middleware function to a route or group in source order.

```dream
public struct UseAttribute
```

## `public struct DepAttribute`

Injects and memoizes a dependency function once per request.

```dream
public struct DepAttribute
```

## `public struct DependencyOverrideAttribute`

Replaces a provider for applications explicitly installing the containing class's dependencies().

```dream
public struct DependencyOverrideAttribute
```

## `public struct ProducesAttribute`

Sets the content type of a yielded string or byte stream.

```dream
public struct ProducesAttribute
```

## `public struct BodyAttribute`

Extracts the request body as text, bytes or a JSON model.

```dream
public struct BodyAttribute
```

## `public struct PathAttribute`

Binds a path value, with an optional name override.

```dream
public struct PathAttribute
```

## `public struct QueryAttribute`

Binds a query value, with an optional name override.

```dream
public struct QueryAttribute
```

## `public struct HeaderAttribute`

Binds a header value, with an optional name override.

```dream
public struct HeaderAttribute
```

## `public struct CookieAttribute`

Binds a cookie value, with an optional name override.

```dream
public struct CookieAttribute
```

## `public struct FormAttribute`

Binds a form value, with an optional name override.

```dream
public struct FormAttribute
```

## `public struct FileAttribute`

Binds a file value, with an optional name override.

```dream
public struct FileAttribute
```

## `public struct SecuritySchemeAttribute`

Declares a dependency's OpenAPI authentication scheme.

```dream
public struct SecuritySchemeAttribute
```

## `public id`

```dream
public id: string
```

## `public kind`

```dream
public kind: string
```

## `public scheme`

```dream
public scheme: string
```

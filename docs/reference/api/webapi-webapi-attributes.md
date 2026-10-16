# Webapi Attributes

**Import:** `import system.webapi;`

For behavior and native target requirements, see [Native Web APIs](../stdlib/webapi.md).

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

## `public struct HttpGroupAttribute`

Prefixes every route declared on a class.

```dream
public struct HttpGroupAttribute
```

## `public struct MiddlewareAttribute`

Installs a global onion layer; lower orders run first.

```dream
public struct MiddlewareAttribute
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

## `public struct WebApiAttribute`

Selects the WebApp declaration for its route generator.

```dream
public struct WebApiAttribute
```

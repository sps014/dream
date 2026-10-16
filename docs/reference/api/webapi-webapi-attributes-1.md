# Webapi Attributes

**Import:** `import system.webapi;`

See the [Web APIs guide](../../web-apis/index.md) for runnable examples, lifecycle, and error behavior.

## `GetAttribute`

Declares an HTTP GET route.

```dream
public struct GetAttribute
```

## `path`

Route path relative to the router prefix; named segments use `{name}`.

```dream
public path: string
```

## `PostAttribute`

Declares an HTTP POST route.

```dream
public struct PostAttribute
```

## `path`

Route path relative to the router prefix; named segments use `{name}`.

```dream
public path: string
```

## `PutAttribute`

Declares an HTTP PUT route.

```dream
public struct PutAttribute
```

## `path`

Route path relative to the router prefix; named segments use `{name}`.

```dream
public path: string
```

## `PatchAttribute`

Declares an HTTP PATCH route.

```dream
public struct PatchAttribute
```

## `path`

Route path relative to the router prefix; named segments use `{name}`.

```dream
public path: string
```

## `DeleteAttribute`

Declares an HTTP DELETE route.

```dream
public struct DeleteAttribute
```

## `path`

Route path relative to the router prefix; named segments use `{name}`.

```dream
public path: string
```

## `HeadAttribute`

Declares an HTTP HEAD route.

```dream
public struct HeadAttribute
```

## `path`

Route path relative to the router prefix; named segments use `{name}`.

```dream
public path: string
```

## `OptionsAttribute`

Declares an HTTP OPTIONS route.

```dream
public struct OptionsAttribute
```

## `path`

Route path relative to the router prefix; named segments use `{name}`.

```dream
public path: string
```

## `WebsocketAttribute`

Declares an HTTP WEBSOCKET route.

```dream
public struct WebsocketAttribute
```

## `path`

Route path relative to the router prefix; named segments use `{name}`.

```dream
public path: string
```

## `RouterAttribute`

Declares an explicitly mounted group of static HTTP handlers.

```dream
public struct RouterAttribute
```

## `prefix`

Path prefix applied to every route in the group.

```dream
public prefix: string
```

## `UseAttribute`

Applies a middleware function to a route or group in source order.

```dream
public struct UseAttribute
```

## `handler`

Middleware function to run for the annotated route or group.

```dream
public handler: DeclarationPath
```

## `DepAttribute`

Injects and memoizes a dependency function once per request.

```dream
public struct DepAttribute
```

## `provider`

Dependency function to evaluate for this parameter or replace for this application.

```dream
public provider: DeclarationPath
```

## `cache`

Whether the dependency result is reused within this request; omitted means true.

```dream
public cache: Option<bool>
```

## `DependencyOverrideAttribute`

Replaces a provider for applications explicitly installing the containing class's dependencies().

```dream
public struct DependencyOverrideAttribute
```

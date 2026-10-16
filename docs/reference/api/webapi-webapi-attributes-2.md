# Webapi Attributes

**Import:** `import system.webapi;`

See the [Web APIs guide](../../web-apis/index.md) for runnable examples, lifecycle, and error behavior.

## `public name`

```dream
public name: Option<string>
```

## `public location`

```dream
public location: Option<string>
```

## `public struct RequiresScopesAttribute`

Requires space-separated permission scopes from a verified CurrentUser dependency.

```dream
public struct RequiresScopesAttribute
```

## `public struct OperationAttribute`

Describes a route operation in OpenAPI.

```dream
public struct OperationAttribute
```

## `public summary`

```dream
public summary: Option<string>
```

## `public description`

```dream
public description: Option<string>
```

## `public tags`

```dream
public tags: Option<string>
```

## `public status`

```dream
public status: Option<int>
```

## `public deprecated`

```dream
public deprecated: Option<bool>
```

## `public struct ResponseAttribute`

Documents an additional explicit response without changing handler behavior.

```dream
public struct ResponseAttribute
```

## `public status`

```dream
public status: int
```

## `public description`

```dream
public description: string
```

## `public model`

```dream
public model: Option<DeclarationPath>
```

## `public content_type`

```dream
public content_type: Option<string>
```

## `public struct OAuth2SchemeAttribute`

Declares an OAuth2 authorization-code scheme for a security dependency.

```dream
public struct OAuth2SchemeAttribute
```

## `public id`

```dream
public id: string
```

## `public authorization_url`

```dream
public authorization_url: string
```

## `public token_url`

```dream
public token_url: string
```

## `public scopes`

```dream
public scopes: Option<string>
```

# Webapi Webapi Attributes 3

**Import:** `import system.webapi;`

## `OperationAttribute`

Describes a route operation in OpenAPI.

```dream
public struct OperationAttribute
```

## `summary`

Short summary displayed in the API documentation.

```dream
public summary: Option<string>
```

## `description`

Description displayed in the API documentation.

```dream
public description: Option<string>
```

## `tags`

Optional tags grouping the operation in the API documentation.

```dream
public tags: Option<string>
```

## `status`

HTTP response status code.

```dream
public status: Option<int>
```

## `deprecated`

Whether the operation is marked deprecated in the API description.

```dream
public deprecated: Option<bool>
```

## `ResponseAttribute`

Documents an additional explicit response without changing handler behavior.

```dream
public struct ResponseAttribute
```

## `status`

HTTP response status code.

```dream
public status: int
```

## `description`

Description displayed in the API documentation.

```dream
public description: string
```

## `model`

Optional declaration describing the response body.

```dream
public model: Option<DeclarationPath>
```

## `content_type`

Media type of the response body.

```dream
public content_type: Option<string>
```

## `OAuth2SchemeAttribute`

Declares an OAuth2 authorization-code scheme for a security dependency.

```dream
public struct OAuth2SchemeAttribute
```

## `id`

Stable identifier of this entry.

```dream
public id: string
```

## `authorization_url`

Endpoint where the user authorizes this application.

```dream
public authorization_url: string
```

## `token_url`

Provider endpoint used to exchange authorization codes for tokens.

```dream
public token_url: string
```

## `scopes`

Space-separated permissions granted to this account or token.

```dream
public scopes: Option<string>
```

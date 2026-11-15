# OAuth2SchemeAttribute

**Import:** `import system.webapi;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `OAuth2SchemeAttribute`

Declares an OAuth2 authorization-code scheme for a security dependency.

```dream
public struct OAuth2SchemeAttribute
```

## `id`

Security scheme name published in the OpenAPI document.

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

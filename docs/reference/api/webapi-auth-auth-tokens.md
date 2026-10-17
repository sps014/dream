# AuthTokens

**Import:** `import system.webapi.auth;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `AuthTokens`

Credentials returned after authentication or refresh. Keep refresh and CSRF tokens secret.

```dream
public class AuthTokens
```

## `access_token`

Signed token sent with authenticated requests.

```dream
public access_token: string
```

## `refresh_token`

Secret used to obtain a new access token.

```dream
public refresh_token: string
```

## `token_type`

Authorization scheme returned with the tokens, usually `Bearer`.

```dream
public token_type: string
```

## `expires_in`

Access token lifetime in seconds.

```dream
public expires_in: int
```

## `csrf_token`

Token the browser must send to protect cookie-authenticated requests.

```dream
public csrf_token: string
```

## `constructor`

Creates a `AuthTokens` with `access_token`, `refresh_token`, `token_type`, `expires_in`, `csrf_token`.

```dream
public constructor(
    access_token: string,
    refresh_token: string,
    token_type: string,
    expires_in: int,
    csrf_token: string
)
```

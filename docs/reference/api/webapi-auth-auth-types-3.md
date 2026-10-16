# Auth Types

**Import:** `import system.webapi.auth;`

See the [Web APIs guide](../../web-server/index.md) for runnable examples, lifecycle, and error behavior.

## `name`

Name identifying this entry.

```dream
public name: string
```

## `issuer`

Identity of the token issuer.

```dream
public issuer: string
```

## `client_id`

Application identifier registered with the identity provider.

```dream
public client_id: string
```

## `client_secret`

Confidential client credential; keep this value secret.

```dream
public client_secret: string
```

## `redirect_uri`

Registered callback URL receiving the authorization response.

```dream
public redirect_uri: string
```

## `constructor`

Creates a `OidcOptions` with `name`, `issuer`, `client_id`, `redirect_uri`, `client_secret`.

```dream
public constructor(
    name: string,
    issuer: string,
    client_id: string,
    redirect_uri: string,
    client_secret: string = ""
)
```

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

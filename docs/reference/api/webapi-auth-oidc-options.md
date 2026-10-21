# OidcOptions

**Import:** `import system.webapi.auth;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `OidcOptions`

One external OIDC identity provider. Redirect URIs must match the registered callback exactly.

```dream
public class OidcOptions
```

## `name`

Provider name used in login routes and to label linked identities.

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

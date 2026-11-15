# AuthSession

**Import:** `import system.webapi.auth;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `AuthSession`

A refresh-token record. Stores contain only the digest of the bearer secret.

```dream
public class AuthSession
```

## `hash`

Hash of the refresh token; the raw refresh token is not stored.

```dream
public hash: string
```

## `family`

Identifier shared by successive refresh tokens for the same session.

```dream
public family: string
```

## `subject`

Stable identifier of the authenticated account.

```dream
public subject: string
```

## `scopes`

Space-separated permissions granted to this account or token.

```dream
public scopes: string
```

## `expires`

Expiry time in Unix epoch seconds.

```dream
public expires: long
```

## `csrf`

Secret used to validate requests made with session cookies.

```dream
public csrf: string
```

## `consumed`

Whether this refresh session has already been rotated.

```dream
public consumed: bool
```

## `constructor`

Creates a `AuthSession` with `hash`, `family`, `subject`, `scopes`, `expires`, `csrf`.

```dream
public constructor(
    hash: string,
    family: string,
    subject: string,
    scopes: string,
    expires: long,
    csrf: string
)
```

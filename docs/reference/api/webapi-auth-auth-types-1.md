# Auth Types

**Import:** `import system.webapi.auth;`

See the [Web APIs guide](../../web-apis/index.md) for runnable examples, lifecycle, and error behavior.

## `AuthError`

Authentication failures carry stable codes without credential values.

```dream
public class AuthError
```

## `code`

Error code identifying the failure.

```dream
public code: string
```

## `message`

Human-readable explanation of the result or failure.

```dream
public message: string
```

## `constructor`

Creates a `AuthError` with `code`, `message`.

```dream
public constructor(code: string, message: string)
```

## `AuthPrincipal`

Verified identity and application-defined space-separated permission scopes.

```dream
public class AuthPrincipal
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

## `constructor`

Creates a `AuthPrincipal` with `subject`, `scopes`.

```dream
public constructor(subject: string, scopes: string)
```

## `has_scope`

Returns whether the space-separated scope list includes `scope`.

```dream
public fun has_scope(scope: string): bool
```

## `AuthAccount`

Persisted account data supplied by an application repository.

```dream
public class AuthAccount
```

## `subject`

Stable identifier of the authenticated account.

```dream
public subject: string
```

## `password_hash`

Encoded password hash; never a plain-text password.

```dream
public password_hash: string
```

## `scopes`

Space-separated permissions granted to this account or token.

```dream
public scopes: string
```

## `constructor`

Creates a `AuthAccount` with `subject`, `password_hash`, `scopes`.

```dream
public constructor(subject: string, password_hash: string, scopes: string = "")
```

## `AuthAccounts`

Local lookup and explicit external-identity linking are application policy.

```dream
public interface AuthAccounts
```

## `find_login`

Looks up an account by login name; returns `AuthError` if authentication cannot proceed.

```dream
async unique fun find_login(login: string): Result<AuthAccount, AuthError>
```

## `link_identity`

Resolves an external identity to a local account, returning an authentication error on failure.

```dream
async unique fun link_identity(
    issuer: string,
    subject: string,
    email: string,
    email_verified: bool
): Result<AuthAccount, AuthError>
```

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

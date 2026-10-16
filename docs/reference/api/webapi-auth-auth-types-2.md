# Auth Types

**Import:** `import system.webapi.auth;`

See the [Web APIs guide](../../web-apis/index.md) for runnable examples, lifecycle, and error behavior.

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

## `OidcTransaction`

An expiring, browser-bound OIDC authorization transaction.

```dream
public class OidcTransaction
```

## `provider`

Name of the identity provider handling this login.

```dream
public provider: string
```

## `nonce`

Random value used to bind an identity response to its login request.

```dream
public nonce: string
```

## `verifier`

PKCE verifier kept until the authorization code is exchanged.

```dream
public verifier: string
```

## `expires`

Expiry time in Unix epoch seconds.

```dream
public expires: long
```

## `constructor`

Creates a `OidcTransaction` with `provider`, `nonce`, `verifier`, `expires`.

```dream
public constructor(provider: string, nonce: string, verifier: string, expires: long)
```

## `AuthSessionStore`

Persistence operations must be atomic, including refresh replay detection and transaction consumption.

```dream
public interface AuthSessionStore
```

## `create`

Stores a new refresh session; returns an error if its token hash already exists.

```dream
async unique fun create(session: AuthSession): Result<bool, AuthError>
```

## `find`

Finds an unexpired session at epoch-second time `now`; rejects revoked or missing sessions.

```dream
async unique fun find(hash: string, now: long): Result<AuthSession, AuthError>
```

## `rotate`

Consumes a refresh session and stores its replacement. Replay revokes the family; invalid replacements return an error.

```dream
async unique fun rotate(
    hash: string,
    replacement: AuthSession,
    now: long
): Result<AuthSession, AuthError>
```

## `revoke`

Revokes all refresh sessions in `family`.

```dream
async unique fun revoke(family: string): Result<bool, AuthError>
```

## `active`

Returns whether `family` has an unexpired, unconsumed session at epoch-second time `now`.

```dream
async unique fun active(family: string, now: long): bool
```

## `put_transaction`

Stores a login transaction under `state`; duplicate state returns an error.

```dream
async unique fun put_transaction(
    state: string,
    transaction: OidcTransaction
): Result<bool, AuthError>
```

## `take_transaction`

Removes and returns the login transaction. Missing or expired state returns an error.

```dream
async unique fun take_transaction(state: string, now: long): Result<OidcTransaction, AuthError>
```

## `JwtOptions`

Native JWT configuration; RS256 uses PEM keys, HS256 requires an explicit 32-byte minimum secret.

```dream
public class JwtOptions
```

## `issuer`

Identity of the token issuer.

```dream
public issuer: string
```

## `audience`

Expected token audience.

```dream
public audience: string
```

## `signing_key`

Private key used to sign tokens; keep it secret.

```dream
public signing_key: string
```

## `verification_key`

Public key used to verify token signatures.

```dream
public verification_key: string
```

## `algorithm`

Signature algorithm name, such as `RS256`.

```dream
public algorithm: string
```

## `constructor`

Creates a `JwtOptions` with `issuer`, `audience`, `signing_key`, `verification_key`, `algorithm`.

```dream
public constructor(
    issuer: string,
    audience: string,
    signing_key: string,
    verification_key: string,
    algorithm: string = "RS256"
)
```

## `OidcOptions`

One external OIDC identity provider. Redirect URIs must match the registered callback exactly.

```dream
public class OidcOptions
```

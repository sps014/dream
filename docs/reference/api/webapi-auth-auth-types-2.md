# Auth Types

**Import:** `import system.webapi.auth;`

See the [Web APIs guide](../../web-apis/index.md) for runnable examples, lifecycle, and error behavior.

## `public consumed`

```dream
public consumed: bool
```

## `public constructor`

```dream
public constructor(hash: string, family: string, subject: string, scopes: string, expires: long, csrf: string)
```

## `public class OidcTransaction`

An expiring, browser-bound OIDC authorization transaction.

```dream
public class OidcTransaction
```

## `public provider`

```dream
public provider: string
```

## `public nonce`

```dream
public nonce: string
```

## `public verifier`

```dream
public verifier: string
```

## `public expires`

```dream
public expires: long
```

## `public constructor`

```dream
public constructor(provider: string, nonce: string, verifier: string, expires: long)
```

## `public interface AuthSessionStore`

Persistence operations must be atomic, including refresh replay detection and transaction consumption.

```dream
public interface AuthSessionStore
```

## `async unique fun create`

```dream
async unique fun create(session: AuthSession): Result<bool, AuthError>
```

## `async unique fun find`

```dream
async unique fun find(hash: string, now: long): Result<AuthSession, AuthError>
```

## `async unique fun rotate`

```dream
async unique fun rotate(hash: string, replacement: AuthSession, now: long): Result<AuthSession, AuthError>
```

## `async unique fun revoke`

```dream
async unique fun revoke(family: string): Result<bool, AuthError>
```

## `async unique fun active`

```dream
async unique fun active(family: string, now: long): bool
```

## `async unique fun put_transaction`

```dream
async unique fun put_transaction(state: string, transaction: OidcTransaction): Result<bool, AuthError>
```

## `async unique fun take_transaction`

```dream
async unique fun take_transaction(state: string, now: long): Result<OidcTransaction, AuthError>
```

## `public class JwtOptions`

Native JWT configuration; RS256 uses PEM keys, HS256 requires an explicit 32-byte minimum secret.

```dream
public class JwtOptions
```

## `public issuer`

```dream
public issuer: string
```

## `public audience`

```dream
public audience: string
```

## `public signing_key`

```dream
public signing_key: string
```

## `public verification_key`

```dream
public verification_key: string
```

## `public algorithm`

```dream
public algorithm: string
```

## `public constructor`

```dream
public constructor(issuer: string, audience: string, signing_key: string, verification_key: string, algorithm: string = "RS256")
```

## `public class OidcOptions`

One external OIDC identity provider. Redirect URIs must match the registered callback exactly.

```dream
public class OidcOptions
```

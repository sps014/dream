# Auth Types

**Import:** `import system.webapi.auth;`

See the [Web APIs guide](../../web-apis/index.md) for runnable examples, lifecycle, and error behavior.

## `public class AuthError`

Authentication failures carry stable codes without credential values.

```dream
public class AuthError
```

## `public code`

```dream
public code: string
```

## `public message`

```dream
public message: string
```

## `public constructor`

```dream
public constructor(code: string, message: string)
```

## `public class AuthPrincipal`

Verified identity and application-defined space-separated permission scopes.

```dream
public class AuthPrincipal
```

## `public subject`

```dream
public subject: string
```

## `public scopes`

```dream
public scopes: string
```

## `public constructor`

```dream
public constructor(subject: string, scopes: string)
```

## `public fun has_scope`

```dream
public fun has_scope(scope: string): bool
```

## `public class AuthAccount`

Persisted account data supplied by an application repository.

```dream
public class AuthAccount
```

## `public subject`

```dream
public subject: string
```

## `public password_hash`

```dream
public password_hash: string
```

## `public scopes`

```dream
public scopes: string
```

## `public constructor`

```dream
public constructor(subject: string, password_hash: string, scopes: string = "")
```

## `public interface AuthAccounts`

Local lookup and explicit external-identity linking are application policy.

```dream
public interface AuthAccounts
```

## `async unique fun find_login`

```dream
async unique fun find_login(login: string): Result<AuthAccount, AuthError>
```

## `async unique fun link_identity`

```dream
async unique fun link_identity(issuer: string, subject: string, email: string, email_verified: bool): Result<AuthAccount, AuthError>
```

## `public class AuthSession`

A refresh-token record. Stores contain only the digest of the bearer secret.

```dream
public class AuthSession
```

## `public hash`

```dream
public hash: string
```

## `public family`

```dream
public family: string
```

## `public subject`

```dream
public subject: string
```

## `public scopes`

```dream
public scopes: string
```

## `public expires`

```dream
public expires: long
```

## `public csrf`

```dream
public csrf: string
```

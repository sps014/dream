# Auth Service

**Import:** `import system.webapi.auth;`

See the [Web APIs guide](../../web-apis/index.md) for runnable examples, lifecycle, and error behavior.

## `public class AuthService`

Local and OIDC authentication with application-owned accounts and atomic session storage.

```dream
public class AuthService
```

## `public constructor`

```dream
public constructor(jwt: JwtOptions, accounts: AuthAccounts, store: AuthSessionStore, prefix: string = "/auth", access_seconds: int = 900, refresh_seconds: int = 604800, browser_sessions: bool = false, browser_origin: string = "")
```

## `public fun add_provider`

```dream
public fun add_provider(options: OidcOptions): Result<bool, AuthError>
```

## `public fun CurrentUser`

Injects identity previously verified by AuthService.middleware().

```dream
public fun CurrentUser(ctx: RequestContext): Result<AuthPrincipal, HttpStatus>
```

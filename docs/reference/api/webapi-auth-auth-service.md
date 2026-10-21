# Auth Service

**Import:** `import system.webapi.auth;`

See the [Web APIs guide](../../web-server/index.md) for runnable examples, lifecycle, and error behavior.

## `AuthService`

Local and OIDC authentication with application-owned accounts and atomic session storage.

```dream
public class AuthService
```

## `constructor`

Creates a `AuthService` with `jwt`, `accounts`, `store`, `prefix`, `access_seconds`, `refresh_seconds`, `browser_sessions`, `browser_origin`.

```dream
public constructor(
    jwt: JwtOptions,
    accounts: AuthAccounts,
    store: AuthSessionStore,
    prefix: string = "/auth",
    access_seconds: int = 900,
    refresh_seconds: int = 604800,
    browser_sessions: bool = false,
    browser_origin: string = ""
)
```

## `add_provider`

Registers an identity provider, returning an error if its configuration is invalid.

```dream
public fun add_provider(options: OidcOptions): Result<Unit, AuthError>
```

## `login`

Verifies local credentials and creates access and refresh tokens. Invalid credentials return `AuthError`.

```dream
public fun login(login: string, password: string): Future<Result<AuthTokens, AuthError>>
```

## `refresh`

Consumes a refresh token and returns replacement tokens. Invalid, expired, or replayed tokens return `AuthError`.

```dream
public fun refresh(token: string): Future<Result<AuthTokens, AuthError>>
```

## `logout`

Revokes the session family associated with the refresh token. Invalid tokens return `AuthError`.

```dream
public fun logout(token: string): Future<Result<Unit, AuthError>>
```

## `verify`

Verifies an access token and its active session, returning the principal or an authentication error.

```dream
public fun verify(token: string): Future<Result<AuthPrincipal, AuthError>>
```

## `middleware`

Installs verified identity into request state. Invalid credentials always short-circuit.

```dream
public fun middleware(): Middleware
```

## `current_user`

Returns the identity that `AuthService.middleware()` verified for this request. Use it as a dependency, `@dep(AuthService.current_user)`. Fails with 401 when the request is not authenticated and 403 when it lacks a scope required by `@requires_scopes`.

```dream
public static fun current_user(ctx: RequestContext): Result<AuthPrincipal, HttpStatus>
```

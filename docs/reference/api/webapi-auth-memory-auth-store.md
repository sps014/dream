# Memory Auth Store

**Import:** `import system.webapi.auth;`

See the [Web APIs guide](../../web-apis/index.md) for runnable examples, lifecycle, and error behavior.

## `public class MemoryAuthStore `

Process-local storage for tests and single-process examples. Methods perform no suspension during mutations.

```dream
public class MemoryAuthStore : AuthSessionStore
```

## `public constructor`

```dream
public constructor()
```

## `public async unique fun create`

```dream
public async unique fun create(session: AuthSession): Result<bool, AuthError>
```

## `public async unique fun find`

```dream
public async unique fun find(hash: string, now: long): Result<AuthSession, AuthError>
```

## `public async unique fun rotate`

```dream
public async unique fun rotate(hash: string, replacement: AuthSession, now: long): Result<AuthSession, AuthError>
```

## `public async unique fun revoke`

```dream
public async unique fun revoke(family: string): Result<bool, AuthError>
```

## `public async unique fun active`

```dream
public async unique fun active(family: string, now: long): bool
```

## `public async unique fun put_transaction`

```dream
public async unique fun put_transaction(state: string, transaction: OidcTransaction): Result<bool, AuthError>
```

## `public async unique fun take_transaction`

```dream
public async unique fun take_transaction(state: string, now: long): Result<OidcTransaction, AuthError>
```

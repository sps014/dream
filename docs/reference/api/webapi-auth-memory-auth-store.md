# Memory Auth Store

**Import:** `import system.webapi.auth;`

See the [Web APIs guide](../../web-server/index.md) for runnable examples, lifecycle, and error behavior.

## `MemoryAuthStore`

Process-local storage for tests and single-process examples. Methods perform no suspension during mutations.

```dream
public class MemoryAuthStore: AuthSessionStore
```

## `constructor`

Creates a `MemoryAuthStore` with default settings.

```dream
public constructor()
```

## `create`

Stores a new refresh session; returns an error if its token hash already exists.

```dream
public async unique fun create(session: AuthSession): Result<Unit, AuthError>
```

## `find`

Finds an unexpired session at epoch-second time `now`; rejects revoked or missing sessions.

```dream
public async unique fun find(hash: string, now: long): Result<AuthSession, AuthError>
```

## `rotate`

Consumes a refresh session and stores its replacement. Replay revokes the family; invalid replacements return an error.

```dream
public async unique fun rotate(
    hash: string,
    replacement: AuthSession,
    now: long
): Result<AuthSession, AuthError>
```

## `revoke`

Revokes all refresh sessions in `family`.

```dream
public async unique fun revoke(family: string): Result<Unit, AuthError>
```

## `active`

Returns whether `family` has an unexpired, unconsumed session at epoch-second time `now`.

```dream
public async unique fun active(family: string, now: long): bool
```

## `put_transaction`

Stores a login transaction under `state`; duplicate state returns an error.

```dream
public async unique fun put_transaction(
    state: string,
    transaction: OidcTransaction
): Result<Unit, AuthError>
```

## `take_transaction`

Removes and returns the login transaction. Missing or expired state returns an error.

```dream
public async unique fun take_transaction(
    state: string,
    now: long
): Result<OidcTransaction, AuthError>
```

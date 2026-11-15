# AuthSessionStore

**Import:** `import system.webapi.auth;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `AuthSessionStore`

Persistence operations must be atomic, including refresh replay detection and transaction consumption.

```dream
public interface AuthSessionStore
```

## `create`

Stores a new refresh session; returns an error if its token hash already exists.

```dream
async unique fun create(session: AuthSession): Result<Unit, AuthError>
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
async unique fun revoke(family: string): Result<Unit, AuthError>
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
): Result<Unit, AuthError>
```

## `take_transaction`

Removes and returns the login transaction. Missing or expired state returns an error.

```dream
async unique fun take_transaction(state: string, now: long): Result<OidcTransaction, AuthError>
```

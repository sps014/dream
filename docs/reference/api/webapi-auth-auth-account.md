# AuthAccount

**Import:** `import system.webapi.auth;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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

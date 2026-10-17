# AuthPrincipal

**Import:** `import system.webapi.auth;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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

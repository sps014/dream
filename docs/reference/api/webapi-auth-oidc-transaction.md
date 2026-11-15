# OidcTransaction

**Import:** `import system.webapi.auth;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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

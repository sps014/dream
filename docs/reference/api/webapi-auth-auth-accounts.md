# AuthAccounts

**Import:** `import system.webapi.auth;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `AuthAccounts`

Local lookup and explicit external-identity linking are application policy.

```dream
public interface AuthAccounts
```

## `find_login`

Looks up an account by login name; returns `AuthError` if authentication cannot proceed.

```dream
async unique fun find_login(login: string): Result<AuthAccount, AuthError>
```

## `link_identity`

Resolves an external identity to a local account, returning an authentication error on failure.

```dream
async unique fun link_identity(
    issuer: string,
    subject: string,
    email: string,
    email_verified: bool
): Result<AuthAccount, AuthError>
```

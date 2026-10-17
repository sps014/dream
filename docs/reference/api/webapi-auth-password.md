# Password

**Import:** `import system.webapi.auth;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Password`

Argon2id password hashes in the interoperable PHC format.

```dream
public static class Password
```

## `hash`

Hashes `password` with Argon2id and returns a PHC-encoded hash, or an authentication error.

```dream
public static fun hash(password: string): Result<string, AuthError>
```

## `verify`

Checks a password against its encoded hash, returning false on mismatch or an error if verification fails.

```dream
public static fun verify(password: string, hash: string): Result<bool, AuthError>
```

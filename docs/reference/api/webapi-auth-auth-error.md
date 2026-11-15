# AuthError

**Import:** `import system.webapi.auth;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `AuthError`

Authentication failures carry stable codes without credential values.

```dream
public class AuthError: Error
```

## `constructor`

Creates an error with a stable `code` such as `EAUTH` and a readable `message`. Neither should contain passwords, tokens, or other credentials.

```dream
public constructor(code: string, message: string)
```

## `code`

Returns the stable machine code, such as `EAUTH` or `EINVAL`.

```dream
public fun code(): string
```

## `message`

Returns the readable explanation of the failure.

```dream
public fun message(): string
```

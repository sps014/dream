# AuthError

**Import:** `import system.webapi.auth;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `AuthError`

Authentication failures carry stable codes without credential values.

```dream
public class AuthError
```

## `code`

Error code identifying the failure.

```dream
public code: string
```

## `message`

Human-readable explanation of the result or failure.

```dream
public message: string
```

## `constructor`

Creates a `AuthError` with `code`, `message`.

```dream
public constructor(code: string, message: string)
```

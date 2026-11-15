# HttpError

**Import:** `import system.net;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `HttpError`

A transport, validation, cancellation, deadline, or capability failure; HTTP statuses are not errors.

```dream
public class HttpError: Error
```

## `constructor`

Creates a failure with a stable machine code and a human-readable description.

```dream
public constructor(code: string, message: string)
```

## `code`

Stable category such as `EINVAL`, `ETIMEOUT`, `ECANCELLED`, `ECLOSED`, or `ENOTSUP`.

```dream
public fun code(): string
```

## `message`

Human-readable failure detail; transport adapters omit request URLs from error messages.

```dream
public fun message(): string
```

# Error

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Error`

Shared contract for fallible stdlib/user errors. Prefer `Result<T, E>` with `E : Error`.

```dream
public interface Error
```

## `message`

Human-readable explanation of the failure, suitable for logs and error output.

```dream
fun message(): string
```

## `code`

Stable machine code (`ENOENT`, `EPARSE`, `HTTP_404`, `EINVAL`, …).

```dream
fun code(): string
```

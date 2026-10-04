# Error

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `interface Error`

Shared contract for fallible stdlib/user errors. Prefer `Result<T, E>` with `E : Error`.

```dream
public interface Error
```

## `message`

Human-readable description.

```dream
fun message(): string
```

## `code`

Stable machine code (`ENOENT`, `EPARSE`, `HTTP_404`, `EINVAL`, …).

```dream
fun code(): string
```

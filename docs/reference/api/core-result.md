# Result

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `enum Result<T, E>`

`Result<T, E>` - the outcome of an operation that may fail.

```dream
public enum Result<T, E>
```

## `Ok(T)`

```dream
Ok(T)
```

## `Err(E)`

```dream
Err(E)
```

## `is_ok`

True when this is `Ok`.

```dream
public fun is_ok(): bool
```

## `is_err`

True when this is `Err`.

```dream
public fun is_err(): bool
```

## `unwrap_or`

The success value when `Ok`, otherwise `fallback`.

```dream
public fun unwrap_or(fallback: T): T
```

## `unwrap`

The success value, or panics if `Err`.

```dream
public fun unwrap(): T
```

## `expect`

The success value, or panics with `msg` if `Err`.

```dream
public fun expect(borrow msg: string): T
```

## `unwrap_or_else`

The success value, or `f(error)` when `Err`.

```dream
public fun unwrap_or_else(borrow f: fun(E): T): T
```

## `unwrap_err`

The error value, or panics if `Ok`.

```dream
public fun unwrap_err(): E
```

## `ok`

`Some` when `Ok`, otherwise `None`.

```dream
public fun ok(): Option<T>
```

## `or_else`

Maps `Err` through `f`, otherwise preserves `Ok`.

```dream
public fun or_else<F>(borrow f: fun(E): Result<T, F>): Result<T, F>
```

## `map`

Maps the success value when `Ok`, otherwise preserves the error.

```dream
public fun map<U>(borrow f: fun(T): U): Result<U, E>
```

## `map_err`

Maps the error when `Err`, otherwise preserves the success value.

```dream
public fun map_err<F>(borrow f: fun(E): F): Result<T, F>
```

## `and_then`

Chains into another `Result` when `Ok`, otherwise preserves the error.

```dream
public fun and_then<U>(borrow f: fun(T): Result<U, E>): Result<U, E>
```

# Option

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `enum Option<T>`

`Option<T>` - a value that is either present (`Some`) or absent (`None`).

```dream
public enum Option<T>
```

## `Some(T)`

```dream
Some(T)
```

## `None`

```dream
None
```

## `is_some`

True when this is `Some`.

```dream
public fun is_some(): bool
```

## `is_none`

True when this is `None`.

```dream
public fun is_none(): bool
```

## `unwrap_or`

The contained value when `Some`, otherwise `fallback`.

```dream
public fun unwrap_or(fallback: T): T
```

## `unwrap`

The contained value, or panics if `None`.

```dream
public fun unwrap(): T
```

## `expect`

The contained value, or panics with `msg` if `None`.

```dream
public fun expect(borrow msg: string): T
```

## `unwrap_or_else`

The contained value, or `f()` when `None`.

```dream
public fun unwrap_or_else(borrow f: fun(): T): T
```

## `filter`

`Some` when the value is present and `pred` is true, otherwise `None`.

```dream
public fun filter(borrow pred: fun(T): bool): Option<T>
```

## `ok_or`

`Ok` when `Some`, otherwise `Err(err)`.

```dream
public fun ok_or<E>(err: E): Result<T, E>
```

## `flatten`

This helper is declared in the library, but calls are currently rejected. See [nested optional values](../stdlib/option-result.md#flatten-nested-optional-values) for a working example using `and_then`.

Collapses `Option<Option<U>>`.

```dream
public static fun flatten<U>(opt: Option<Option<U>>): Option<U>
```

## `map`

Maps the contained value when `Some`, otherwise `None`.

```dream
public fun map<U>(borrow f: fun(T): U): Option<U>
```

## `and_then`

Chains into another `Option` when `Some`, otherwise `None`.

```dream
public fun and_then<U>(borrow f: fun(T): Option<U>): Option<U>
```

## `or`

This option when `Some`, otherwise `fallback`.

```dream
public fun or(fallback: Option<T>): Option<T>
```

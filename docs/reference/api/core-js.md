# Js

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `null`

```dream
public static get null(): js
```

## `undefined`

```dream
public static get undefined(): js
```

## `to_int`

Explicit conversions back to Dream values.

```dream
public fun to_int(): int
```

## `to_double`

```dream
public fun to_double(): double
```

## `to_bool`

```dream
public fun to_bool(): bool
```

## `to_str`

```dream
public fun to_str(): string
```

## `is_null`

True if the referenced value is `null` or `undefined`.

```dream
public fun is_null(): bool
```

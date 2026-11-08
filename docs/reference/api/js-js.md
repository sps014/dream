# Js

**Import:** `import system.js;`

Read the [JavaScript interop guide](../../language/interop/index.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `object`

Creates an empty JavaScript object whose properties can be assigned dynamically.

```dream
public static fun object(): js
```

## `array`

Creates an empty JavaScript array.

```dream
public static fun array(): js
```

## `null`

The JavaScript `null` value.

```dream
public static get null(): js
```

## `undefined`

The JavaScript `undefined` value.

```dream
public static get undefined(): js
```

## `to_int`

Converts this JavaScript value to a Dream integer.

```dream
public fun to_int(): int
```

## `to_double`

Converts this JavaScript value to a Dream double.

```dream
public fun to_double(): double
```

## `to_bool`

Converts this JavaScript value to a Dream boolean.

```dream
public fun to_bool(): bool
```

## `to_str`

Converts this JavaScript value to a Dream string.

```dream
public fun to_str(): string
```

## `is_null`

True if the referenced value is `null` or `undefined`.

```dream
public fun is_null(): bool
```

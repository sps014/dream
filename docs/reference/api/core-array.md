# Array

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Array`

Creates fixed-length arrays whose elements can be accessed with zero-based indices.

```dream
public static class Array
```

## `alloc`

Creates an array of `len` default-initialized elements.

```dream
public static fun alloc<T>(len: int): T[]
```

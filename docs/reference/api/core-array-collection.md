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

## `repeat`

A `T[]` of length `len` whose every slot holds `value` (evaluated once, shared).

```dream
public static fun repeat<T>(len: int, value: T): T[]
```

## `repeat_with`

Like `repeat`, but calls `fill()` once per slot so each element can be a fresh value (`[v; n]` uses this whenever `v` itself constructs an array, giving distinct rows).

```dream
public static fun repeat_with<T>(len: int, fill: fun(): T): T[]
```

## `length`

Builtin array length (resolved as `ArrayLen` before instance dispatch).

```dream
public get length(): int
```

## `this`

Element at `index`. Panics if `index` is out of range.

```dream
public fun this[index: int]: T
```

## `iterator`

Enumerator for treating an array as a `Collection`.

```dream
public fun iterator(): Iterator<T>
```

## `contains`

Returns whether the collection contains the requested key or value.

```dream
public fun contains(borrow value: T): bool
```

## `index_of`

Returns the first matching index, or `None` when no match exists.

```dream
public fun index_of(borrow value: T): Option<int>
```

## `slice`

Copies the range from `start` (inclusive) to `end` (exclusive); invalid bounds panic.

```dream
public fun slice(start: int, end: int): T[]
```

## `reverse`

Reverses the elements in place.

```dream
public fun reverse(): void
```

## `concat`

Returns a new array containing this array followed by `other`.

```dream
public fun concat(borrow other: T[]): T[]
```

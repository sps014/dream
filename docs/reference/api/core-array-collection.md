# Array

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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

Returns a new array copied from `start` (inclusive) to `end` (exclusive). Panics unless `0 <= start <= end <= length`.

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

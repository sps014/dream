# Array

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `static class Array`

`Array.alloc<T>(n)` — user-facing zero-initialized `T[]` allocation.

```dream
public static class Array
```

## `alloc`

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

```dream
public fun contains(borrow value: T): bool
```

## `index_of`

```dream
public fun index_of(borrow value: T): Option<int>
```

## `slice`

```dream
public fun slice(start: int, end: int): T[]
```

## `reverse`

```dream
public fun reverse(): void
```

## `concat`

```dream
public fun concat(borrow other: T[]): T[]
```

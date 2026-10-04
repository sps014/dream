# ScratchArena

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class ScratchArena<T : unmanaged>`

Bump-pointer scratch slab for short-lived unmanaged batches (parse/match/fill loops). Parameterized on `T : unmanaged` so the same arena shape works for `int`, `byte`, `char`, unmanaged structs, etc. Use one arena per element type (or several arenas) in a hot loop. Overflow stops the program; grow by constructing a larger arena.

```dream
public class ScratchArena<T : unmanaged>
```

## `constructor`

Allocates a zeroed slab of `capacity` elements (at least one).

```dream
public constructor(capacity: int)
```

## `remaining`

Number of elements still available before the next `reset`.

```dream
public get remaining(): int
```

## `capacity`

Total element capacity of the slab.

```dream
public get capacity(): int
```

## `alloc`

Bump-allocates `n` elements and returns a `Span` over them. Does not zero the span (callers that need zeros should write them); prior contents after `reset` may be stale. Prefer `bump`/`set_at`/`at` in hot loops: a `Span` holds a strong ref to the slab, so each `alloc` pays retain/release on `items`.

```dream
public fun alloc(n: int): Span<T>
```

## `bump`

```dream
public fun bump(n: int): int
```

## `set_at`

Absolute-index write into the slab (same addressing as `bump`'s return value).

```dream
public fun set_at(index: int, value: T): void
```

## `at`

Absolute-index read from the slab.

```dream
public fun at(index: int): T
```

## `reset`

Rewinds the bump cursor; the slab stays allocated.

```dream
public fun reset(): void
```

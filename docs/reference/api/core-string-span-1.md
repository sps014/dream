# StringSpan

No import is needed.

Read the [usage guide](../language/spans.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](core-string-span.md)

## `ref struct StringSpan`

`StringSpan` - a read-only view of `length` UTF-16 units of a `string`, starting at `offset`. It is a `ref struct`: stored inline with no heap allocation of its own, and rejected anywhere it could outlive the frame that made it. It keeps its source string strongly referenced, so the units it views stay alive; `to_string()` is the one place it allocates.

```dream
public ref struct StringSpan
```

## `of`

A span over the whole of `s`.

```dream
public static fun of(borrow s: string): StringSpan
```

## `length`

Number of UTF-16 units in the view.

```dream
public get length(): int
```

## `is_empty`

```dream
public fun is_empty(): bool
```

## `byte_size`

Payload bytes of the view (two per unit).

```dream
public fun byte_size(): int
```

## `char_at`

The unit at `index` (relative to the view). Traps if out of range.

```dream
public fun char_at(index: int): char
```

## `this`

```dream
public fun this[index: int]: char
```

## `byte_at`

Payload byte `index` (UTF-16 LE, relative to the view). Traps if out of range.

```dream
public fun byte_at(index: int): int
```

## `slice`

The sub-view `[start, end)` of this view, clamped exactly like `string.substring`.

```dream
public fun slice(start: int, end: int): StringSpan
```

## `slice`

The view from `start` to the end.

```dream
public fun slice(start: int): StringSpan
```

## `to_string`

Copies the viewed units into a `string` (an O(1) slice of the source).

```dream
public override fun to_string(): string
```

## `hash_code`

Same value as `this.to_string().hash_code()`, without allocating.

```dream
public override fun hash_code(): int
```

## `equals`

True when the view holds exactly the units of `other`.

```dream
public fun equals(borrow other: string): bool
```

## `compare`

Code-unit order against `other`: negative / zero / positive.

```dream
public fun compare(other: StringSpan): int
```

## `compare`

```dream
public fun compare(borrow other: string): int
```

## `starts_with`

```dream
public fun starts_with(borrow prefix: string): bool
```

## `ends_with`

```dream
public fun ends_with(borrow suffix: string): bool
```

## `contains`

```dream
public fun contains(borrow sub: string): bool
```

## `contains`

```dream
public fun contains(c: char): bool
```

## `index_of`

Index (relative to the view) of the first `c`, or `None`.

```dream
public fun index_of(c: char): Option<int>
```

## `index_of`

```dream
public fun index_of(c: char, from: int): Option<int>
```

## `index_of`

Index (relative to the view) of the first occurrence of `sub`, or `None`.

```dream
public fun index_of(borrow sub: string): Option<int>
```

## `index_of`

```dream
public fun index_of(borrow sub: string, from: int): Option<int>
```

## `last_index_of`

```dream
public fun last_index_of(c: char): Option<int>
```

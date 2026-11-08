# Short

No import is needed.

Read the [usage guide](../../language/types/primitives.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

At the C boundary, `short` is `int16_t`.

## `compare`

Orders this value against `other`.

```dream
public fun compare(other: short): int
```

## `min`

The smaller of this and `other`.

```dream
public fun min(other: short): short
```

## `max`

The larger of this and `other`.

```dream
public fun max(other: short): short
```

## `clamp`

This value constrained to the inclusive range [lo, hi].

```dream
public fun clamp(lo: short, hi: short): short
```

## `parse`

Parses a signed decimal `short` from `text`.

```dream
public static fun parse(borrow text: string): Result<short, ParseError>
```

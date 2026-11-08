# SByte

No import is needed.

Read the [usage guide](../../language/types/primitives.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

At the C boundary, `sbyte` is `int8_t`.

## `compare`

Orders this value against `other`.

```dream
public fun compare(other: sbyte): int
```

## `min`

The smaller of this and `other`.

```dream
public fun min(other: sbyte): sbyte
```

## `max`

The larger of this and `other`.

```dream
public fun max(other: sbyte): sbyte
```

## `clamp`

This value constrained to the inclusive range [lo, hi].

```dream
public fun clamp(lo: sbyte, hi: sbyte): sbyte
```

## `parse`

Parses a signed decimal `sbyte` from `text`.

```dream
public static fun parse(borrow text: string): Result<sbyte, ParseError>
```

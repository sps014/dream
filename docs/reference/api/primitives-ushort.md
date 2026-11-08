# UShort

No import is needed.

Read the [usage guide](../../language/types/primitives.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

At the C boundary, `ushort` is `uint16_t`.

## `compare`

Orders this value against `other`.

```dream
public fun compare(other: ushort): int
```

## `min`

The smaller of this and `other`.

```dream
public fun min(other: ushort): ushort
```

## `max`

The larger of this and `other`.

```dream
public fun max(other: ushort): ushort
```

## `clamp`

This value constrained to the inclusive range [lo, hi].

```dream
public fun clamp(lo: ushort, hi: ushort): ushort
```

## `parse`

Parses an unsigned decimal `ushort` from `text`.

```dream
public static fun parse(borrow text: string): Result<ushort, ParseError>
```

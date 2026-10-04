# Uint

No import is needed.

Read the [usage guide](../language/primitives.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `compare`

Orders this value against `other`.

```dream
public fun compare(other: uint): int
```

## `min`

The smaller of this and `other`.

```dream
public fun min(other: uint): uint
```

## `max`

The larger of this and `other`.

```dream
public fun max(other: uint): uint
```

## `clamp`

This value constrained to the inclusive range [lo, hi].

```dream
public fun clamp(lo: uint, hi: uint): uint
```

## `parse`

Parses an unsigned decimal integer from `text`.

```dream
public static fun parse(borrow text: string): Result<uint, ParseError>
```

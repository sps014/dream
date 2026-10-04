# Ulong

No import is needed.

Read the [usage guide](../language/primitives.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `compare`

Orders this value against `other`.

```dream
public fun compare(other: ulong): int
```

## `min`

The smaller of this and `other`.

```dream
public fun min(other: ulong): ulong
```

## `max`

The larger of this and `other`.

```dream
public fun max(other: ulong): ulong
```

## `clamp`

This value constrained to the inclusive range [lo, hi].

```dream
public fun clamp(lo: ulong, hi: ulong): ulong
```

## `parse`

Parses an unsigned decimal integer from `text`.

```dream
public static fun parse(borrow text: string): Result<ulong, ParseError>
```

# Byte

No import is needed.

Read the [usage guide](../language/primitives.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `compare`

Orders this value against `other`.

```dream
public fun compare(other: byte): int
```

## `min`

The smaller of this and `other`.

```dream
public fun min(other: byte): byte
```

## `max`

The larger of this and `other`.

```dream
public fun max(other: byte): byte
```

## `clamp`

This value constrained to the inclusive range [lo, hi].

```dream
public fun clamp(lo: byte, hi: byte): byte
```

## `parse`

Parses an unsigned decimal byte from `text`.

```dream
public static fun parse(borrow text: string): Result<byte, ParseError>
```

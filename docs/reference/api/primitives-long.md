# Long

No import is needed.

Read the [usage guide](../language/primitives.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `compare`

Orders this value against `other`.

```dream
public fun compare(other: long): int
```

## `abs`

Absolute value of this long.

```dream
public fun abs(): long
```

## `min`

The smaller of this and `other`.

```dream
public fun min(other: long): long
```

## `max`

The larger of this and `other`.

```dream
public fun max(other: long): long
```

## `clamp`

This value constrained to the inclusive range [lo, hi].

```dream
public fun clamp(lo: long, hi: long): long
```

## `signum`

The sign of this value: -1, 0, or 1.

```dream
public fun signum(): long
```

## `parse`

Parses a decimal integer from `text`.

```dream
public static fun parse(borrow text: string): Result<long, ParseError>
```

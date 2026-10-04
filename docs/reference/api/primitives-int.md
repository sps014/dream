# Int

No import is needed.

Read the [usage guide](../language/primitives.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `compare`

Orders this value against `other`: negative when less, zero when equal, positive when greater. Implementing `Comparable<int>` lets `List<int>.sort()` / `binary_search` work out of the box.

```dream
public fun compare(other: int): int
```

## `abs`

Absolute value of this integer.

```dream
public fun abs(): int
```

## `min`

The smaller of this and `other`.

```dream
public fun min(other: int): int
```

## `max`

The larger of this and `other`.

```dream
public fun max(other: int): int
```

## `clamp`

This value constrained to the inclusive range [lo, hi].

```dream
public fun clamp(lo: int, hi: int): int
```

## `pow`

This value raised to a non-negative integer power (exponents <= 0 yield 1).

```dream
public fun pow(exp: int): int
```

## `signum`

The sign of this value: -1, 0, or 1.

```dream
public fun signum(): int
```

## `parse`

Parses a (possibly signed) decimal integer from `text`.

```dream
public static fun parse(borrow text: string): Result<int, ParseError>
```

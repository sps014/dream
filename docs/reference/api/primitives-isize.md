# Isize

No import is needed.

Read the [usage guide](../../language/types/primitives.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `compare`

Returns a negative number, zero, or a positive number for less than, equal to, or greater than `other`.

```dream
public fun compare(other: isize): int
```

## `abs`

Returns the magnitude of this value. The most negative `isize` has no positive counterpart and is returned unchanged.

```dream
public fun abs(): isize
```

## `signum`

Returns -1, 0, or 1 for a negative, zero, or positive value.

```dream
public fun signum(): isize
```

## `min`

Returns the smaller of this value and `other`.

```dream
public fun min(other: isize): isize
```

## `max`

Returns the larger of this value and `other`.

```dream
public fun max(other: isize): isize
```

## `clamp`

Returns the value limited to the inclusive range `lo` through `hi`.

```dream
public fun clamp(lo: isize, hi: isize): isize
```

## `parse`

Parses the text as this numeric type; returns `ParseError` if invalid or out of range.

```dream
public static fun parse(borrow text: string): Result<isize, ParseError>
```

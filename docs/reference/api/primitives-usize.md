# Usize

No import is needed.

Read the [usage guide](../language/primitives.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `compare`

Returns a negative number, zero, or a positive number for less than, equal to, or greater than `other`.

```dream
public fun compare(other: usize): int
```

## `min`

Returns the smaller of this value and `other`.

```dream
public fun min(other: usize): usize
```

## `max`

Returns the larger of this value and `other`.

```dream
public fun max(other: usize): usize
```

## `clamp`

Returns the value limited to the inclusive range `lo` through `hi`.

```dream
public fun clamp(lo: usize, hi: usize): usize
```

## `parse`

Parses the text as this numeric type; returns `ParseError` if invalid or out of range.

```dream
public static fun parse(borrow text: string): Result<usize, ParseError>
```

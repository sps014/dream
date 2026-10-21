# Double

No import is needed.

Read the [usage guide](../../language/types/primitives.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `compare`

Orders this value against `other`.

```dream
public fun compare(other: double): int
```

## `abs`

Returns the magnitude of this value with the sign cleared, so `-0.0` becomes `0.0` and NaN stays NaN.

```dream
public fun abs(): double
```

## `min`

Returns the smaller of this value and `other`. When either is NaN, returns `other`.

```dream
public fun min(other: double): double
```

## `max`

Returns the larger of this value and `other`. When either is NaN, returns `other`.

```dream
public fun max(other: double): double
```

## `clamp`

Returns this value limited to the inclusive range `lo` through `hi`. NaN is returned unchanged; the result is unspecified when `lo` is greater than `hi`.

```dream
public fun clamp(lo: double, hi: double): double
```

## `parse`

Parses a decimal double from `text`.

```dream
public static fun parse(borrow text: string): Result<double, ParseError>
```

# Double

No import is needed.

Read the [usage guide](../language/primitives.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `compare`

Orders this value against `other`.

```dream
public fun compare(other: double): int
```

## `abs`

Absolute value of this double.

```dream
public fun abs(): double
```

## `min`

The smaller of this and `other`.

```dream
public fun min(other: double): double
```

## `max`

The larger of this and `other`.

```dream
public fun max(other: double): double
```

## `parse`

Parses a decimal double from `text`.

```dream
public static fun parse(borrow text: string): Result<double, ParseError>
```

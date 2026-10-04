# Isize

No import is needed.

Read the [usage guide](../language/primitives.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `compare`

```dream
public fun compare(other: isize): int
```

## `min`

```dream
public fun min(other: isize): isize
```

## `max`

```dream
public fun max(other: isize): isize
```

## `clamp`

```dream
public fun clamp(lo: isize, hi: isize): isize
```

## `parse`

```dream
public static fun parse(borrow text: string): Result<isize, ParseError>
```

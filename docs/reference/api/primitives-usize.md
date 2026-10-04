# Usize

No import is needed.

Read the [usage guide](../language/primitives.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `compare`

```dream
public fun compare(other: usize): int
```

## `min`

```dream
public fun min(other: usize): usize
```

## `max`

```dream
public fun max(other: usize): usize
```

## `clamp`

```dream
public fun clamp(lo: usize, hi: usize): usize
```

## `parse`

```dream
public static fun parse(borrow text: string): Result<usize, ParseError>
```

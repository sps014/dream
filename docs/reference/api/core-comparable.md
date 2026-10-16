# Comparable

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Comparable`

Total order: `compare` returns negative / zero / positive like `strcmp`.

```dream
public interface Comparable<T>
```

## `compare`

Returns a negative number, zero, or a positive number for less than, equal to, or greater than `other`.

```dream
fun compare(borrow other: T): int
```

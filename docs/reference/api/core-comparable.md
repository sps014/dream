# Comparable

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `interface Comparable<T>`

Total order: `compare` returns negative / zero / positive like `strcmp`.

```dream
public interface Comparable<T>
```

## `compare`

```dream
fun compare(borrow other: T): int
```

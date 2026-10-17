# Iterable

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Iterable`

A sequence that creates a fresh synchronous cursor for each traversal.

```dream
public interface Iterable<T>
```

## `iterator`

Creates a cursor over the sequence. Close it when iteration ends early.

```dream
borrow fun iterator(): Iterator<T>
```

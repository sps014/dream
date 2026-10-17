# AsyncIterable

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `AsyncIterable`

A sequence that creates a fresh asynchronous cursor for each traversal.

```dream
public interface AsyncIterable<T>
```

## `iterator`

Creates a cursor over the sequence. Close it when iteration ends early.

```dream
borrow fun iterator(): AsyncIterator<T>
```

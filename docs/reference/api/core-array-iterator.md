# ArrayIterator

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class ArrayIterator<T> : Iterator<T>`

Cursor produced by `T[].iterator()` when an array participates as `Collection` / `IndexedCollection`. Concrete `for (let x in arr)` still uses the native index loop.

```dream
public class ArrayIterator<T> : Iterator<T>
```

## `next`

Returns the next element, or `None` if exhausted.

```dream
public fun next(): Option<T>
```

# ListIterator

**Import:** `import system.collections;`

Read the [usage guide](../stdlib/collections.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class ListIterator<T> : Iterator<T>`

Cursor produced by `List.iterator()`.

```dream
public class ListIterator<T> : Iterator<T>
```

## `next`

Returns the next element, or `None` if exhausted.

```dream
public fun next(): Option<T>
```

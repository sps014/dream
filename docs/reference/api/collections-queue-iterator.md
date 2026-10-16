# QueueIterator

**Import:** `import system.collections;`

Read the [usage guide](../../library/collections/index.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `QueueIterator`

Cursor produced by `Queue.iterator()`.

```dream
public class QueueIterator<T>: Iterator<T>
```

## `next`

Returns the next element, or `None` if exhausted.

```dream
public fun next(): Option<T>
```

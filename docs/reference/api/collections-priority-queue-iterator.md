# PriorityQueueIterator

**Import:** `import system.collections;`

Read the [usage guide](../stdlib/collections.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class PriorityQueueIterator<T> : Iterator<T>`

Cursor produced by `PriorityQueue.iterator()`. Yields elements in ascending order under the active comparator). Iteration is destructive-free: it drains into a snapshot first, so the queue can be mutated while the cursor is alive.

```dream
public class PriorityQueueIterator<T> : Iterator<T>
```

## `next`

Returns the next element, or `None` if exhausted.

```dream
public fun next(): Option<T>
```

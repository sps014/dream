# PriorityQueue

**Import:** `import system.collections;`

Read the [usage guide](../stdlib/collections/priority-queue.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class PriorityQueue<T> : Collection<T>`

Binary min-heap: `pop`/`peek` always return the smallest element by the active comparator (natural ascending order via `Comparable<T>` by default, or a custom `fun(T, T): int`).

```dream
public class PriorityQueue<T> : Collection<T>
```

## `constructor`

Empty queue ordered by `T`'s natural `Comparable` order.

```dream
public constructor()
```

## `constructor`

Empty queue ordered by a custom comparator (same contract as `List.sort_by`: negative when `a` should come out first, positive when `b` should).

```dream
public constructor(cmp: fun(T, T): int)
```

## `length`

Number of elements currently stored.

```dream
public get length(): int
```

## `is_empty`

True when the queue has no elements.

```dream
public fun is_empty(): bool
```

## `clear`

Removes every element. Live slots are overwritten with a zero value so managed elements release immediately instead of staying retained until later pushes overwrite them.

```dream
public fun clear(): void
```

## `clear`

```dream
public fun clear(): void where T : unmanaged
```

## `push`

Inserts `value`, restoring the heap invariant. O(log n).

```dream
public fun push(value: T): void
```

## `pop`

Removes and returns the smallest element, or `None` when empty. O(log n).

```dream
public fun pop(): Option<T>
```

## `peek`

Returns the smallest element without removing it, or `None` when empty. O(1).

```dream
public fun peek(): Option<T>
```

## `iterator`

Iterates elements in ascending order under the active comparator, over a snapshot taken now, so later pushes/pops do not affect the cursor.

```dream
public fun iterator(): PriorityQueueIterator<T>
```

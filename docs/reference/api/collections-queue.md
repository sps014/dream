# Queue

**Import:** `import system.collections;`

Read the [usage guide](../stdlib/collections/queue.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class Queue<T> : Collection<T>`

FIFO queue backed by a ring buffer (`T[]` + head/count). Invariant: `items.length > 0`, `0 <= head < items.length`, `0 <= count <= items.length`, so any `(head + k) % items.length` with `k >= 0` is a valid slot for `Buffer.get_unchecked`.

```dream
public class Queue<T> : Collection<T>
```

## `constructor`

Creates an empty queue.

```dream
public constructor()
```

## `length`

Number of elements currently stored.

```dream
public get length(): int
```

## `enqueue`

Appends `value` at the back of the queue.

```dream
public fun enqueue(value: T): void
```

## `dequeue`

Removes and returns the front element, or `None` when empty. The vacated slot is zeroed so managed elements release immediately instead of staying retained until overwritten.

```dream
public fun dequeue(): Option<T>
```

## `peek`

Returns the front element without removing it, or `None` when empty.

```dream
public fun peek(): Option<T>
```

## `clear`

Removes every element. Live ring slots are overwritten with a zero value so managed elements release immediately instead of staying retained until later enqueues overwrite them; the ring is reset to front-aligned empty.

```dream
public fun clear(): void
```

## `clear`

```dream
public fun clear(): void where T : unmanaged
```

## `iterator`

Iterates elements from front to back.

```dream
public fun iterator(): QueueIterator<T>
```

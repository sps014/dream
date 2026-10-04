# Stack

**Import:** `import system.collections;`

Read the [usage guide](../stdlib/collections/stack.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class Stack<T> : Collection<T>`

LIFO stack backed by `List<T>`.

```dream
public class Stack<T> : Collection<T>
```

## `constructor`

Creates an empty stack.

```dream
public constructor()
```

## `length`

Number of elements currently stored.

```dream
public get length(): int
```

## `push`

Pushes `value` onto the top of the stack.

```dream
public fun push(value: T): void
```

## `pop`

Removes and returns the top element, or `None` when empty.

```dream
public fun pop(): Option<T>
```

## `peek`

Returns the top element without removing it, or `None` when empty.

```dream
public fun peek(): Option<T>
```

## `clear`

Removes every element.

```dream
public fun clear(): void
```

## `iterator`

Iterates elements from bottom to top.

```dream
public fun iterator(): ListIterator<T>
```

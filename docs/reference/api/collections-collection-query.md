# Collection Query

**Import:** `import system.collections;`

Read the [usage guide](../stdlib/collections.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `to_list`

Copies every element into a new `List`.

```dream
public fun to_list(): List<T>
```

## `filter`

Keeps elements for which `pred` is true.

```dream
public fun filter(borrow pred: fun(T): bool): List<T>
```

## `map`

Maps each element through `f`.

```dream
public fun map<U>(borrow f: fun(T): U): List<U>
```

## `reduce`

Left-fold with seed `init`.

```dream
public fun reduce<A>(init: A, borrow f: fun(A, T): A): A
```

## `collect_set`

Inserts every element into a new `Set` (deduplicating).

```dream
public fun collect_set(): Set<T>
```

## `distinct`

Removes duplicates while preserving first-seen order.

```dream
public fun distinct(): List<T>
```

## `flat_map`

Maps each element to a list and concatenates the results.

```dream
public fun flat_map<U>(borrow f: fun(T): List<U>): List<U>
```

## `take`

Keeps at most the first `n` elements.

```dream
public fun take(n: int): List<T>
```

## `skip`

Drops the first `n` elements.

```dream
public fun skip(n: int): List<T>
```

## `order_by`

Copies into a list and sorts by `cmp`.

```dream
public fun order_by(borrow cmp: fun(T, T): int): List<T>
```

## `seq`

Starts an eager `Seq` chain over a copy of the elements.

```dream
public fun seq(): Seq<T>
```

## `min`

Smallest element by `compare`, or `None` when empty.

```dream
public fun min(): Option<T> where T : Comparable<T>
```

## `max`

Largest element by `compare`, or `None` when empty.

```dream
public fun max(): Option<T> where T : Comparable<T>
```

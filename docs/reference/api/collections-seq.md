# Seq

**Import:** `import system.collections;`

Read the [usage guide](../../library/collections/seq.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Seq`

A repeatable query sequence whose filters and transformations run during traversal.

```dream
public class Seq<T>: Iterable<T>
```

## `iterator`

Creates an independent cursor. Closing it also closes every upstream cursor.

```dream
public borrow fun iterator(): Iterator<T>
```

## `filter`

Lazily keeps the elements for which `p` returns true.

```dream
public fun filter(borrow p: fun(T): bool): Seq<T>
```

## `take`

Lazily keeps at most `n` elements; nonpositive counts produce an empty sequence.

```dream
public fun take(n: int): Seq<T>
```

## `skip`

Lazily skips the first `n` elements; nonpositive counts skip nothing.

```dream
public fun skip(n: int): Seq<T>
```

## `to_list`

Collects the surviving elements into a fresh list.

```dream
public fun to_list(): List<T>
```

## `count`

Counts the surviving elements without building a list.

```dream
public fun count(): int
```

## `distinct`

Lazily removes duplicates while preserving first-seen order.

```dream
public fun distinct(): Seq<T>
```

## `order_by`

Immediately collects and sorts the sequence with `cmp`.

```dream
public fun order_by(borrow cmp: fun(T, T): int): Seq<T>
```

## `flat_map`

Lazily maps each element to a list and visits each result in order.

```dream
public fun flat_map<U>(borrow f: fun(T): List<U>): Seq<U>
```

## `map`

Lazily transforms each element through `f`.

```dream
public fun map<U>(borrow f: fun(T): U): Seq<U>
```

## `reduce`

Combines elements in order, starting with `init`; returns `init` when empty.

```dream
public fun reduce<A>(init: A, borrow f: fun(A, T): A): A
```

## `collect_set`

Collects the elements into a new set, removing duplicates.

```dream
public fun collect_set(): Set<T>
```

## `min`

Returns the smallest element by `compare`, or `None` when empty.

```dream
public fun min(): Option<T> where T: Comparable<T>
```

## `max`

Returns the largest element by `compare`, or `None` when empty.

```dream
public fun max(): Option<T> where T: Comparable<T>
```

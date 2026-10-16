# Seq

**Import:** `import system.collections;`

Read the [usage guide](../stdlib/collections/seq.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Seq`

A lazy sequence of values that can be transformed and iterated.

```dream
public class Seq<T>
```

## `filter`

Keeps elements for which `pred` is true (deferred — runs once per element at materialization).

```dream
public fun filter(borrow p: fun(T): bool): Seq<T>
```

## `take`

Keeps the first `n` surviving elements of the chain (deferred).

```dream
public fun take(n: int): Seq<T>
```

## `skip`

Drops the first `n` surviving elements of the chain (deferred).

```dream
public fun skip(n: int): Seq<T>
```

## `to_list`

Terminal step: dense copy of the surviving elements, in order.

```dream
public fun to_list(): List<T>
```

## `count`

Terminal step: number of elements that survive the chain.

```dream
public fun count(): int
```

## `distinct`

Removes duplicates while preserving first-seen order.

```dream
public fun distinct(): Seq<T>
```

## `order_by`

Returns a new sequence sorted by `cmp`.

```dream
public fun order_by(borrow cmp: fun(T, T): int): Seq<T>
```

## `flat_map`

Maps each element to a list and concatenates the results.

```dream
public fun flat_map<U>(borrow f: fun(T): List<U>): Seq<U>
```

## `map`

Terminal step: maps the surviving elements through `f` into a new `Seq`.

```dream
public fun map<U>(borrow f: fun(T): U): Seq<U>
```

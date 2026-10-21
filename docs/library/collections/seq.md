---
title: Queries and Seq
summary: "Filter, transform, and combine collection items."
icon: material/filter-outline
api: reference/api/system-collections.md
---

# Query and transform collections

Start a `Seq<T>` with `collection.seq()` to keep, transform, count, or combine collection elements. Build the chain, then request a list or another final result.

**Import:** `import system.collections;` · **Availability:** desktop, browser, and Node.

```dream
import system;
import system.collections;

fun main() {
    let numbers: List<int> = [1, 2, 3, 4, 5];
    let selected = numbers.seq().filter((n) => n > 2).take(2).to_list();
    System.println(selected.join(", "));
}
```

Output: `3, 4`.

## Immediate queries

The common `Collection<T>` interface provides the helpers in the first table. These do not require the collections package by themselves. `List`, `Map`, `Set`, `Queue`, `Stack`, and `PriorityQueue` are all collections; a map's items are its `KeyValuePair<K, V>` entries.

| Method | Returns | Effect |
| --- | --- | --- |
| `is_empty()` | `bool` | `true` when there are no items. |
| `all(pred)` | `bool` | Whether `pred` is `true` for every item; `true` when empty. |
| `any(pred)` | `bool` | Whether `pred` is `true` for at least one item. |
| `none(pred)` | `bool` | Whether `pred` is `false` for every item. |
| `count(pred)` | `int` | Number of items where `pred` is `true`. |
| `find(pred)` | `Option<T>` | First item where `pred` is `true`. |
| `for_each(action)` | `void` | Calls `action` on each item in loop order. |

Lists also provide `first()`, `last()`, and `find_index(pred)`; see [List](list.md).

## Sequence chains

`collection.seq()` starts from a copy of the collection's elements. On the sequence, `filter`, `map`, `flat_map`, `distinct`, `take`, and `skip` build steps that run when a result is requested. `to_list()` copies the selected elements into a list, and `count()` counts them.

Each traversal runs these steps again, without intermediate lists, and `take` stops upstream work once it has enough elements. `sort_by` immediately collects and sorts all elements. Shared objects inside the copied elements remain shared.

| Method | Returns | When it runs |
| --- | --- | --- |
| `filter(pred)` | `Seq<T>` | Delayed: keeps items where `pred` is `true`. |
| `take(n)` | `Seq<T>` | Delayed: keeps at most the first `n` remaining items. |
| `skip(n)` | `Seq<T>` | Delayed: drops the first `n` remaining items. |
| `map(f)` | `Seq<U>` | Delayed: transforms each item as it is requested. |
| `flat_map(f)` | `Seq<U>` | Delayed: visits the elements of each list `f` returns. |
| `distinct()` | `Seq<T>` | Delayed: removes duplicates, keeping first-seen order. |
| `sort_by(compare)` | `Seq<T>` | Immediately: runs the chain and sorts the result. |
| `to_list()` | `List<T>` | Runs the chain and returns the remaining items. |
| `count()` | `int` | Runs the chain and returns how many items remain. |

A `Seq` can also finish with `reduce(initial, combine)`, `collect_set()`, `min()`, or `max()`.
`min` and `max` require comparable elements and return `None` for an empty sequence.
`reduce` returns the initial value when there are no elements.
Traverse a `Seq` directly with a `for` loop. Call `to_list()` when you need a list to index or change.

```dream
import system;
import system.collections;

fun main(): void {
    let numbers: List<int> = [1, 2, 3, 4];
    let total = numbers.seq().filter((n) => n > 2).reduce(0, (sum, n) => sum + n);
    System.println(total);
}
```

This prints `7`. A query over an empty list returns `0` for the same reduction.

See [Seq signatures](../../reference/api/collections-seq.md), [query helper signatures](../../reference/api/collections-collection-seq.md), and [common collection methods](../../reference/api/core-collection.md).

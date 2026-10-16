---
title: Queries and Seq
summary: "Filter, transform, and combine collection items."
icon: material/filter-outline
api: reference/api/system-collections.md
---

# Query and transform collections

Use query helpers to keep, transform, count, or combine collection elements. Ordinary helpers return their result immediately. A `Seq<T>` lets you build a chain before producing its final result.

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
| `count_where(pred)` | `int` | Number of items where `pred` is `true`. |
| `find_where(pred)` | `Option<T>` | First item where `pred` is `true`. |
| `for_each(action)` | `void` | Calls `action` on each item in loop order. |

Import the collections package for the helpers below. These produce their result during the call. `filter` keeps matching items, while `map` changes each item into a new value.

| Method | Returns | Effect |
| --- | --- | --- |
| `to_list()` | `List<T>` | Copies the items into a new list. |
| `filter(pred)` | `List<T>` | Items where `pred` is `true`. |
| `map(f)` | `List<U>` | Result of `f` for each item. |
| `flat_map(f)` | `List<U>` | Lists returned by `f`, joined into one list. |
| `reduce(init, f)` | `A` | Starts with `init` and combines each item with `f(total, item)`. |
| `collect_set()` | `Set<T>` | Distinct items as a set. |
| `distinct()` | `List<T>` | Items with duplicates removed, keeping first-seen order. |
| `take(n)` | `List<T>` | At most the first `n` items. |
| `skip(n)` | `List<T>` | Items after the first `n`. |
| `order_by(compare)` | `List<T>` | Sorted copy, using the same comparison rule as `List.sort_by`. |
| `min()` | `Option<T>` | Smallest item; requires `T: Comparable<T>`. |
| `max()` | `Option<T>` | Largest item; requires `T: Comparable<T>`. |
| `seq()` | `Seq<T>` | Starts a sequence chain over a copy of the items. |

Lists also provide `first()`, `last()`, and `find_index_where(pred)`; see [List](list.md).

## Sequence chains

`collection.seq()` starts from a copy of the collection's elements. On the sequence, `filter`, `take`, and `skip` build steps that run when a result is requested. `to_list()` copies the selected elements into a list, and `count()` counts them.

`map`, `flat_map`, `distinct`, and `order_by` produce their intermediate results immediately. A sequence is therefore not a promise that every operation is delayed. Shared objects inside the copied elements remain shared.

| Method | Returns | When it runs |
| --- | --- | --- |
| `filter(pred)` | `Seq<T>` | Delayed: keeps items where `pred` is `true`. |
| `take(n)` | `Seq<T>` | Delayed: keeps at most the first `n` remaining items. |
| `skip(n)` | `Seq<T>` | Delayed: drops the first `n` remaining items. |
| `map(f)` | `Seq<U>` | Immediately: runs the chain and transforms each item. |
| `flat_map(f)` | `Seq<U>` | Immediately: runs the chain and joins the lists `f` returns. |
| `distinct()` | `Seq<T>` | Immediately: runs the chain and removes duplicates, keeping first-seen order. |
| `order_by(compare)` | `Seq<T>` | Immediately: runs the chain and sorts the result. |
| `to_list()` | `List<T>` | Runs the chain and returns the remaining items. |
| `count()` | `int` | Runs the chain and returns how many items remain. |

A `Seq` has no `for` loop support or other query helpers of its own. Call `to_list()` to get a list you can loop over or query further.

See [Seq signatures](../../reference/api/collections-seq.md), [query helper signatures](../../reference/api/collections-collection-query.md), and [common collection methods](../../reference/api/core-collection.md).

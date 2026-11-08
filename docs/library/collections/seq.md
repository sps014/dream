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

The `Collection<T>` helpers in the first table do not require the collections package. `List`, `Map`, `Set`, `Queue`, `Stack`, and `PriorityQueue` are collections, and a map's items are `KeyValuePair<K, V>` entries.

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

`collection.seq()` starts from a copy of the elements. `filter`, `map`, `flat_map`, `distinct`, `take`, and `skip` run when you ask for a result; `to_list()` copies what remains and `count()` counts it.

Each pass runs the steps again, and `take` stops once it has enough elements. `sort_by` sorts immediately, and objects inside the copied elements stay shared.

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

A `Seq` can also finish with `reduce(initial, combine)`, `collect_set()`, `min()`, or `max()`. `min` and `max` need comparable elements and return `None` when empty; `reduce` returns the initial value when there are no elements.

A `for` loop walks a `Seq` directly. Call `to_list()` when you need to index or change the items.

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

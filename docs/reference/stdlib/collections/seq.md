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

The common `Collection<T>` interface provides `all`, `any`, `none`, `count_where`, `find_where`, `for_each`, and `is_empty`. These do not require the collections package by themselves.

Import the collections package for `to_list`, `filter`, `map`, `reduce`, `collect_set`, `distinct`, `flat_map`, `take`, `skip`, and `order_by`. These produce their result during the call. `filter` keeps matching items, while `map` changes each item into a new value.

## Sequence chains

`collection.seq()` starts from a copy of the collection's elements. On the sequence, `filter`, `take`, and `skip` build steps that run when a result is requested. `to_list()` copies the selected elements into a list, and `count()` counts them.

`map`, `flat_map`, `distinct`, and `order_by` produce their intermediate results immediately. A sequence is therefore not a promise that every operation is delayed. Shared objects inside the copied elements remain shared.

See [Seq signatures](../../api/collections-seq.md), [query helper signatures](../../api/collections-collection-query.md), and [common collection methods](../../api/core-collection.md).

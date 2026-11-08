---
title: SortedMap
summary: A map that keeps its keys in order.
icon: material/sort-ascending
api: reference/api/system-collections.md
---

# SortedMap: keys in order

`SortedMap<K, V>` keeps keys in comparison order, for the first, last, or nearest key as well as ordinary lookups. Keys must support `Comparable<K>`.

**Import:** `import system.collections;` · **Availability:** desktop, browser, and Node.

```dream
import system;
import system.collections;

fun main() {
    let scores = SortedMap<string, int>();
    scores["Sam"] = 80;
    scores["Ada"] = 95;
    for (let entry in scores) {
        System.println(entry.key);
    }
}
```

Output:

```text
Ada
Sam
```

## Read and change

`set` adds or replaces a value. `get(key)` returns `Option<V>`, while `map[key]` requires the key; `contains`, `remove`, `clear`, `.length`, and `is_empty()` cover the rest.

`first_key()` and `last_key()` return `Option<K>`. `ceiling_key` is the smallest key at or above the one you pass, and `floor_key` is the largest at or below it; both return `None` when nothing qualifies.

`keys()` and `values()` return arrays in key order. Iteration produces key-value pairs in that same order.

| Member | Returns | Effect |
| --- | --- | --- |
| `SortedMap<K, V>(capacity = 8)` | `SortedMap<K, V>` | Empty map with room for `capacity` entries. |
| `map[key] = value`, `set(key, value)` | `void` | Adds the key or replaces its value. |
| `get(key)` | `Option<V>` | Value for `key`, or `None`. |
| `map[key]` | `V` | Value for `key`; a missing key stops the program. |
| `contains(key)` | `bool` | Whether `key` is present. |
| `remove(key)` | `bool` | Deletes the entry; `false` when the key was missing. |
| `clear()` | `void` | Deletes every entry and keeps capacity. |
| `.length` | `int` | Number of entries. |
| `is_empty()` | `bool` | `true` when there are no entries. |
| `first_key()` | `Option<K>` | Smallest key. |
| `last_key()` | `Option<K>` | Largest key. |
| `ceiling_key(key)` | `Option<K>` | Smallest key greater than or equal to `key`. |
| `floor_key(key)` | `Option<K>` | Largest key less than or equal to `key`. |
| `keys()` | `K[]` | New array of keys, smallest first. |
| `values()` | `V[]` | New array of values, in key order. |

Lookups are fast, but adding or removing a key moves the entries after it. For a large map that changes often and does not need order, a [Map](map.md) is faster.

`SortedMap` is not a `Collection`, so the [query helpers](seq.md) are not on it. Copy `keys()` or `values()` into a list first.

See [all SortedMap signatures](../../reference/api/collections-sorted-map.md) and [Map](map.md) for ordinary key lookup.

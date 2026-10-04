# SortedMap: keys in order

`SortedMap<K, V>` keeps keys in their comparison order. Use it when you need the first, last, or nearest key as well as ordinary lookups. Keys must support `Comparable<K>`.

**Import:** `import system.collections;` · **Availability:** desktop, browser, and Node.

```dream
import system;
import system.collections;

fun main() {
    let scores = SortedMap<string, int>();
    scores.set("Sam", 80);
    scores.set("Ada", 95);
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

`set` adds or replaces a value. `get(key)` returns `Option<V>`, while indexed reading requires the key to exist. `contains`, `remove`, `clear`, `.length`, and `is_empty()` cover common operations.

`first_key()` and `last_key()` return `Option<K>`. `ceiling_key(key)` finds the smallest key greater than or equal to the supplied key. `floor_key(key)` finds the largest key less than or equal to it. Both return `None` if no key qualifies.

`keys()` and `values()` return arrays in key order. Iteration produces key-value pairs in that same order.

See [all SortedMap signatures](../../api/collections-sorted-map.md) and [Map](map.md) for ordinary key lookup.

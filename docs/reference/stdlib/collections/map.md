# Map: look up values by key

A `Map<K, V>` associates a key with a value. Use it for scores by player, settings by name, or records by ID.

**Import:** `import system.collections;` · **Availability:** desktop, browser, and Node.

```dream
import system;
import system.collections;

fun main() {
    let scores: Map<string, int> = {"Ada": 95};
    scores.set("Sam", 80);
    switch (scores.get("Lee")) {
        Some(score) => System.println(score),
        None => System.println("No score yet"),
    }
}
```

This prints `No score yet`.

## Add and look up

Create a map with `Map<K, V>()`, an optional initial capacity, or a typed map literal. `set(key, value)` adds a key or replaces its value. `map[key] = value` does the same.

`get(key)` returns `Option<V>`. Indexed reading, `map[key]`, requires the key to exist and stops the program if it is missing. Use `get_or(key, fallback)` when you want a default. `get_or_insert(key, factory)` calls a no-argument function to create and store a value only when the key is missing.

## Combine and inspect

`set_all(keys, values)` adds entries from two arrays, pairing matching positions and using the shorter array's length. `from_arrays(keys, values)` creates a map that way. `merge(other)` copies another map's entries into the current map. Existing entries with matching keys are replaced.

Read `.length`, test `contains(key)`, and use `remove(key)` or `clear()` to delete entries. `keys()`, `values()`, and `entries()` expose contents. A `for` loop gives `KeyValuePair<K, V>` values with `.key` and `.value`.

For sorted keys, choose [SortedMap](sorted-map.md). See [all Map signatures](../../api/collections-map.md).

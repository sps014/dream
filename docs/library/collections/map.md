---
title: Map
summary: Look up values by key.
icon: material/map-outline
api: reference/api/system-collections.md
---

# Map: look up values by key

A `Map<K, V>` associates a key with a value. Use it for scores by player, settings by name, or records by ID.

**Import:** `import system.collections;` · **Availability:** desktop, browser, and Node.

```dream
import system;
import system.collections;

fun main() {
    let scores: Map<string, int> = {
        "Ada": 95
    };
    scores["Sam"] = 80;
    switch (scores.get("Lee")) {
        Some(score) => System.println(score),
        None => System.println("No score yet"),
    }
}
```

This prints `No score yet`.

## Add and look up

Create a map with `Map<K, V>()`, an optional initial capacity, or a typed map literal. `set(key, value)` adds a key or replaces its value. `map[key] = value` does the same.

`get(key)` returns `Option<V>`. Indexed reading, `map[key]`, requires the key to exist and stops the program if it is missing. Use `get_or(key, fallback)` when you want a default. `get_or_insert(key, factory)` borrows the key for the lookup, and calls a no-argument function to create and store a value only when the key is missing (copying the key on that miss).

| Member | Returns | Effect |
| --- | --- | --- |
| `Map<K, V>(capacity = 8)` | `Map<K, V>` | Empty map sized for `capacity` entries. |
| `map[key] = value`, `set(key, value)` | `void` | Adds the key or replaces its value. |
| `get(key)` | `Option<V>` | Value for `key`, or `None`. |
| `map[key]` | `V` | Value for `key`; a missing key stops the program. |
| `get_or(key, fallback)` | `V` | Value for `key`, or `fallback` when missing. |
| `get_or_insert(key, factory)` | `V` | Existing value, or stores and returns `factory()`. |
| `contains(key)` | `bool` | Whether `key` is present. |
| `remove(key)` | `bool` | Deletes the entry; `false` when the key was missing. |
| `clear()` | `void` | Deletes every entry and keeps storage. |

When the key type is `string`, `get`, `get_or`, and `contains` also accept a `ReadOnlySpan<char>`, so you can look up text you have not turned into a `string`.

## Combine and inspect

`set_all(keys, values)` adds entries from two arrays, pairing matching positions and using the shorter array's length. `from_arrays(keys, values)` creates a map that way. `merge(other)` copies another map's entries into the current map. Existing entries with matching keys are replaced.

`keys()`, `values()`, and `entries()` return new copies, so changing the map later does not change them. A `for` loop gives `KeyValuePair<K, V>` values with `.key` and `.value`. Entries come out in no guaranteed order.

| Member | Returns | Effect |
| --- | --- | --- |
| `Map.from_arrays(keys, values)` | `Map<K, V>` | New map pairing matching array positions. |
| `set_all(keys, values)` | `void` | Adds or replaces entries from two arrays. |
| `merge(other)` | `void` | Copies `other`'s entries, replacing matching keys. |
| `.length` | `int` | Number of entries. |
| `.capacity` | `int` | Current table size. The map grows before the table is three-quarters full. |
| `is_empty()` | `bool` | `true` when there are no entries. |
| `reserve(additional)` | `Result<bool, CapacityError>` | Makes room for `additional` more entries; `Err` for a negative or unsupported count. |
| `keys()` | `K[]` | Array of the keys. |
| `values()` | `V[]` | Array of the values. |
| `entries()` | `List<KeyValuePair<K, V>>` | List of key-value pairs. |
| `to_string()` | `string` | Entries as text in the form `{key: value, key: value}`. |

A map is a collection of `KeyValuePair<K, V>` values, so the [query helpers](seq.md) such as `filter` and `any` work on its entries.

For sorted keys, choose [SortedMap](sorted-map.md). See [all Map signatures](../../reference/api/collections-map.md).

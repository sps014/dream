---
title: Set
summary: Keep unique values and test membership.
icon: material/set-center
api: reference/api/system-collections.md
---

# Set: keep unique values

A `Set<T>` stores each distinct value once. Use it to remove duplicates or check membership.

**Import:** `import system.collections;` · **Availability:** desktop, browser, and Node.

```dream
import system;
import system.collections;

fun main() {
    let names: Set<string> = {
        "Ada", "Sam"
    };
    names.add("Ada");
    System.println(names.length);
    System.println(names.contains("Sam"));
}
```

Output: `2`, then `true` on a new line. Adding an existing value does not add a second copy.

## Add, check, and remove

`add` returns `true` only when the value was new, so it also tells you whether you have seen a value before. A `for` loop visits each value once, in no guaranteed order.

| Member | Returns | Effect |
| --- | --- | --- |
| `Set<T>(capacity = 8)` | `Set<T>` | Empty set sized for `capacity` values. |
| `Set.from_array(items)` | `Set<T>` | New set of the array's distinct values. Set literals work the same way. |
| `add(value)` | `bool` | Adds the value; `false` when it was already present. |
| `add_all(items)` | `void` | Adds every value of an array, ignoring duplicates. |
| `contains(value)` | `bool` | Whether the value is present. |
| `remove(value)` | `bool` | Removes the value; `false` when it was missing. |
| `clear()` | `void` | Removes every value and keeps storage. |
| `.length` | `int` | Number of values. |
| `.capacity` | `int` | Current table size. The set grows before the table is three-quarters full. |
| `is_empty()` | `bool` | `true` when the set has no values. |
| `reserve(additional)` | `Result<bool, CapacityError>` | Makes room for `additional` more values; `Err` for a negative or unsupported count. |
| `to_array()` | `T[]` | New array of the values. |
| `to_string()` | `string` | Values as text in the form `{a, b}`. |

For a `Set<string>`, `contains` also accepts a `ReadOnlySpan<char>`, so you can test text you have not turned into a `string`.

## Compare sets

`union(other)` includes values from either set. `intersection(other)` keeps values in both. `difference(other)` keeps values only in this set. `is_subset` and `is_disjoint` answer relationship questions.

| Member | Returns | Effect |
| --- | --- | --- |
| `union(other)` | `Set<T>` | New set of values in either set. |
| `intersection(other)` | `Set<T>` | New set of values in both sets. |
| `difference(other)` | `Set<T>` | New set of values in this set but not `other`. |
| `is_subset(other)` | `bool` | Whether every value of this set is in `other`. |
| `is_disjoint(other)` | `bool` | Whether the sets share no values. |

Sets also have the [query helpers](seq.md) such as `filter`, `map`, and `any`.

Use a list if positions or duplicate values matter. See [all Set signatures](../../reference/api/collections-set.md).

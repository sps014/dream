---
title: List
summary: A growable sequence you read and change by position.
icon: material/format-list-numbered
api: reference/api/system-collections.md
---

# List: items in order

A `List<T>` grows as you add items. Positions start at zero. Use it when order matters and you want to read or change an item by position.

**Import:** `import system.collections;` · **Availability:** desktop, browser, and Node.

```dream
import system;
import system.collections;

fun main() {
    let values: List<int> = [3, 1, 2];
    values.push(4);
    values.sort();
    System.println(values.join(", "));
}
```

Output: `1, 2, 3, 4`.

## Create and grow

Use `List<T>()`, `List<T>(capacity)`, or a typed list literal. Capacity reserves room; it does not create that many items. `reserve(additional)` makes room for that many more items beyond the current length. It returns `Err(CapacityError)` for a negative count or a total too large to store, and leaves the list unchanged in that case. Read the item count with `.length` and reserved room with `.capacity`.

`insert(index, value)` adds an item at a position and moves later items. The index may equal `.length` to append.

| Member | Returns | Effect |
| --- | --- | --- |
| `List<T>(capacity = 8)` | `List<T>` | Empty list with room for `capacity` items (at least 4). |
| `List(items)` | `List<T>` | New list holding the array's items in order. List literals work the same way. |
| `.length` | `int` | Number of items. |
| `.capacity` | `int` | Items that fit before the list must grow. |
| `is_empty()` | `bool` | `true` when the list has no items. |
| `reserve(additional)` | `Result<Unit, CapacityError>` | Makes room for `additional` more items. |
| `push(value)` | `void` | Adds an item at the end. |
| `push_all(items)` | `void` | Appends every item of an array, in order. |
| `insert(index, value)` | `bool` | Inserts at `index`; `false` when `index` is outside 0 through `.length`. |

## Read and remove

`list[index]` requires a valid position and stops the program if the position is outside the list. Indexed assignment replaces an item.

`pop()` returns `Option<T>` because a list can be empty. `remove_at(index)` returns whether it removed an item; `remove(value)` removes the first matching item. Searches compare items by value, so strings match by their contents.

| Member | Returns | Effect |
| --- | --- | --- |
| `list[index]` | `T` | Item at `index`; an invalid index stops the program. |
| `list[index] = value` | `void` | Replaces the item; an invalid index stops the program. |
| `first()` | `Option<T>` | First item, or `None` when empty. |
| `last()` | `Option<T>` | Last item, or `None` when empty. |
| `contains(value)` | `bool` | Whether an equal item is present. |
| `index_of(value)` | `Option<int>` | Position of the first equal item. |
| `last_index_of(value)` | `Option<int>` | Position of the last equal item. |
| `find_index(pred)` | `Option<int>` | Position of the first item where `pred` returns `true`. |
| `pop()` | `Option<T>` | Removes and returns the last item. |
| `remove_at(index)` | `bool` | Removes the item at `index`; `false` and no change for an invalid index. |
| `remove(value)` | `bool` | Removes the first equal item; `false` when none matched. |
| `clear()` | `void` | Removes every item and keeps capacity. |

## Copy, join, and sort

`clone()` makes a new list holding the same elements. Objects inside it are still shared; it does not copy each object. `slice(start, end)` makes a list from a range, excluding `end`. It panics unless `0 <= start <= end <= length`, the same rule as array and span `slice`. `concat(other)` makes a combined list. `reverse()` changes the current list.

`join(separator)` converts items to text and joins them. `sort()` uses the elements' comparison order. `sort_by(compare)` takes your comparison function: return a negative number when the first item should come first, zero when equal, or a positive number when the second should come first. Sorting is stable, so equal items keep their relative order. `binary_search` requires a list already sorted in the matching order.

`take_array()` returns an array containing the items and leaves the list empty. `clear()` removes every item while keeping reserved capacity.

| Member | Returns | Effect |
| --- | --- | --- |
| `clone()` | `List<T>` | New list with the same items. |
| `slice(start, end)` | `List<T>` | New list of items from `start` up to, but not including, `end`. |
| `concat(other)` | `List<T>` | New list of this list's items followed by `other`'s. |
| `reverse()` | `void` | Reverses the list in place. |
| `sort()` | `void` | Sorts ascending; requires `T: Comparable<T>`. |
| `sort_by(compare)` | `void` | Sorts with your comparison function. |
| `binary_search(value)` | `Option<int>` | Position of an equal item in an ascending sorted list; requires `T: Comparable<T>`. |
| `join(separator)` | `string` | Items as text, separated by `separator`. |
| `to_string()` | `string` | Items as text in the form `[1, 2, 3]`. |
| `take_array()` | `T[]` | Moves the items into an array and empties the list. |
| `as_span()` | `Span<T>` | Writable view of the current items. |
| `as_read_only_span()` | `ReadOnlySpan<T>` | Read-only view of the current items. |

A span keeps showing the storage it was taken from. After the list grows, shrinks, or is emptied, take a new span.

A `for` loop visits items from first to last. Use `list.seq()` to start the transformations described in [queries](seq.md). Collection predicates such as `any` are also available directly.

See [all List signatures](../../reference/api/collections-list.md), [queries](seq.md), and [collection choice](index.md).

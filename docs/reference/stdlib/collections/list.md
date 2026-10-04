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

Use `List<T>()`, `List<T>(capacity)`, or a typed list literal. Capacity reserves room; it does not create that many items. `reserve(n)` makes capacity at least `n`. Read the item count with `.length` and reserved room with `.capacity`.

`push(value)` adds one item. `push_all(array)` appends an array's items. `List.from_array(array)` creates a list from an array. `insert(index, value)` adds an item at a position and moves later items.

## Read and remove

`list[index]` and `get(index)` require a valid position. Unlike map `get`, list `get` returns the item directly and stops the program for an invalid position. `set` or indexed assignment replaces an item.

`pop()` returns `Option<T>` because a list can be empty. `remove_at(index)` returns whether it removed an item; `remove(value)` removes the first matching item. `contains`, `index_of`, and `last_index_of` help you search.

## Copy, join, and sort

`clone()` makes a new list holding the same elements. Objects inside it are still shared; it does not copy each object. `slice(start, end)` makes a list from a range, excluding `end`. `concat(other)` makes a combined list. `reverse()` changes the current list.

`join(separator)` converts items to text and joins them. `sort()` uses the elements' comparison order. `sort_by(compare)` takes your comparison function: return a negative number when the first item should come first, zero when equal, or a positive number when the second should come first. `binary_search` requires a list already sorted in the matching order.

`take_array()` returns an array containing the items and leaves the list empty. `clear()` removes every item while keeping reserved capacity.

See [all List signatures](../../api/collections-list.md), [queries](seq.md), and [collection choice](../collections.md).

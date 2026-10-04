# Set: keep unique values

A `Set<T>` stores each distinct value once. Use it to remove duplicates or check membership.

**Import:** `import system.collections;` · **Availability:** desktop, browser, and Node.

```dream
import system;
import system.collections;

fun main() {
    let names: Set<string> = {"Ada", "Sam"};
    names.add("Ada");
    System.println(names.length);
    System.println(names.contains("Sam"));
}
```

Output: `2`, then `true` on a new line. Adding an existing value does not add a second copy.

`add`, `remove`, `contains`, and `clear` change or check membership. Read `.length` for the count or call `is_empty()`.

`union(other)` includes values from either set. `intersection(other)` keeps values in both. `difference(other)` keeps values only in this set. `is_subset` and `is_disjoint` answer relationship questions.

Use a list if positions or duplicate values matter. See [all Set signatures](../../api/collections-set.md).

# Range collection conversion

**Import:** `import system.collections;`

Use `(5..0).to_list()` to create a list containing `5, 4, 3, 2, 1`. The list has independent storage. See [Ranges](../../language/types/ranges.md) for a runnable example.

## `to_list`

Copies the range into a new list in iteration order. Panics if its length exceeds int.

```dream
public fun to_list(): List<int>
```

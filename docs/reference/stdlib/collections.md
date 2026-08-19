# Collections

**Import:** `import system.collections;`

Growable `List<T>`, `Map<K, V>`, `Set<T>`, `Queue<T>`, and `Stack<T>`. All have `.length` and work with `for..in`.

```dream
import system;
import system.collections;

fun main() {
    let nums: List<int> = [1, 2, 3];
    nums.push(4);

    let users: Set<string> = {"alice", "bob"};
    let scores: Map<string, int> = {"alice": 95, "bob": 80};

    for (let n in nums) {
        System.println(n);
    }
}
```

- `[1, 2, 3]` is a `List` only when the expected type is `List<T>`; otherwise it is an array (`int[]`).
- `{a, b}` is a `Set`. `{k: v}` is a `Map`.
- Empty `[]` / `{}` need a type: `let xs: Set<int> = {};`.

## Query helpers

Always available (no import): `all`, `any`, `none`, `count_where`, `find_where`, `for_each`, `is_empty`.

Need this package: `filter`, `map`, and related helpers that return a `List`.

## `List<T>`

`List<T>()`, `List.with_capacity(n)`, or a typed `[…]` literal.

| Area | Calls |
| --- | --- |
| Size | `.length`, `is_empty()` |
| Change | `push`, `pop`, `insert`, `remove_at`, `clear`, `set`, `reverse`, `concat`, `slice` |
| Read | `get`, `[]`, `index_of`, `last_index_of`, `contains`, `join` |
| Order | `sort`, `binary_search` |

### Sorting and search

`List<T : Comparable<T>>.sort()` and `List<T>.sort_by(cmp)` use `compare` from [interfaces](../language/interfaces.md). `binary_search` needs a sorted list.

## `Map<K, V>`

Typed `{k: v, …}` literal, or `Map<K, V>()`.

| Area | Calls |
| --- | --- |
| Size | `.length`, `is_empty()` |
| Change | `set`, `remove`, `clear` |
| Read | `get` → `Option<V>`, `contains`, `get_or`, `get_or_insert`, `keys`, `values`, `entries` |

`for (let pair in map)` yields `KeyValuePair<K, V>` (`.key`, `.value`).

## `Set<T>`

Typed `{e, …}` literal, or `Set<T>()`.

`add`, `remove`, `contains`, `clear`, `.length`, `is_empty()`, plus `union` / `intersection` / `difference` / `is_subset` / `is_disjoint`.

## `Queue<T>` / `Stack<T>`

Queue: `enqueue` / `dequeue` / `peek` (FIFO). Stack: `push` / `pop` / `peek` (LIFO).

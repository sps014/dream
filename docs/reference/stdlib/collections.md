# Choose a collection

Collections hold several values. Choose the type based on how you will read and change those values.

**Import:** `import system.collections;`

| You need… | Use |
| --- | --- |
| Items you can read by position | [List](collections/list.md) |
| Values you look up by a key | [Map](collections/map.md) |
| Keys kept in sorted order | [SortedMap](collections/sorted-map.md) |
| Unique values | [Set](collections/set.md) |
| Items processed in arrival order | [Queue](collections/queue.md) |
| The most recently added item first | [Stack](collections/stack.md) |
| The smallest or highest-priority item first | [PriorityQueue](collections/priority-queue.md) |
| Filtering and transforming items | [Queries and Seq](collections/seq.md) |

## Start with a list

```dream
import system;
import system.collections;

fun main() {
    let names: List<string> = ["Ada", "Sam"];
    names.push("Lee");
    for (let name in names) {
        System.println(name);
    }
}
```

This prints each name on a separate line. The `List<string>` type tells Dream to make a growable list. Without that expected type, square brackets create a fixed [array](../language/arrays.md).

Typed `{value, value}` creates a set, and typed `{key: value}` creates a map. Empty collections need a type, such as `let names: Set<string> = {};`.

## Missing items and positions

Check each collection's result type before assuming an item exists. Map lookups and queue removals can return `None`. List indexing uses a position and stops the program if that position is outside the list.

See [Option and Result](option-result.md) for handling absence and [the API catalog](../api/system-collections.md) for all public members.

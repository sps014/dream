---
title: Collections
summary: "Lists, maps, sets, queues, stacks, and queries over them."
icon: material/view-grid-outline
api: reference/api/system-collections.md
---

# Choose a collection

Collections hold several values. Choose the type based on how you will read and change those values.

**Import:** `import system.collections;`

| You need… | Use |
| --- | --- |
| Items you can read by position | [List](list.md) |
| Values you look up by a key | [Map](map.md) |
| Keys kept in sorted order | [SortedMap](sorted-map.md) |
| Unique values | [Set](set.md) |
| Items processed in arrival order | [Queue](queue.md) |
| The most recently added item first | [Stack](stack.md) |
| The smallest or highest-priority item first | [PriorityQueue](priority-queue.md) |
| Filtering and transforming items | [Queries and Seq](seq.md) |

## Guides in this section

<!-- pages -->

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

This prints each name on its own line. The `List<string>` type makes a growable list; without it, square brackets create a fixed [array](../../language/types/arrays.md).

Typed `{value, value}` creates a set, and typed `{key: value}` creates a map. Empty collections need a type, such as `let names: Set<string> = {};`.

## Missing items and positions

Check the result type before assuming an item exists. Map lookups and queue removals can return `None`, and a list index outside the list stops the program.

See [Option and Result](../core/option-result.md) for handling absence and [the API catalog](../../reference/api/system-collections.md) for all public members.

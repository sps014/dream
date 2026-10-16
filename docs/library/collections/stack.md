---
title: Stack
summary: Remove the most recently added item first.
icon: material/layers-outline
api: reference/api/system-collections.md
---

# Stack: remove the newest item first

A stack removes the most recently added item first. Use it for undo steps or work that should be handled in reverse order.

**Import:** `import system.collections;` · **Availability:** desktop, browser, and Node.

```dream
import system;
import system.collections;

fun main() {
    let steps = Stack<string>();
    steps.push("first");
    steps.push("second");
    System.println(steps.pop().unwrap_or("empty"));
}
```

This prints `second`. `push` adds an item. `pop` removes the newest item and returns `Option<T>`. `peek` reads it without removing it. An empty stack returns `None`.

| Member | Returns | Effect |
| --- | --- | --- |
| `Stack<T>()` | `Stack<T>` | Empty stack. |
| `push(value)` | `void` | Adds an item on top. |
| `pop()` | `Option<T>` | Removes and returns the top item. |
| `peek()` | `Option<T>` | Top item, left on the stack. |
| `clear()` | `void` | Removes every item. |
| `.length` | `int` | Number of items. |
| `is_empty()` | `bool` | `true` when the stack has no items. |

A `for` loop visits items from the bottom (oldest) to the top (newest) without removing them. Stacks also have the [query helpers](seq.md) such as `filter` and `any`.

See [all Stack signatures](../../reference/api/collections-stack.md) and [Queue](queue.md) for arrival-order processing.

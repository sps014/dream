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

Use `.length`, `is_empty()`, and `clear()` to inspect or reset it. See [all Stack signatures](../../api/collections-stack.md) and [Queue](queue.md) for arrival-order processing.

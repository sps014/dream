# Queue: process items in arrival order

A queue removes the oldest item first. Use it for work waiting to be processed.

**Import:** `import system.collections;` · **Availability:** desktop, browser, and Node.

```dream
import system;
import system.collections;

fun main() {
    let jobs = Queue<string>();
    jobs.enqueue("first");
    jobs.enqueue("second");
    System.println(jobs.dequeue().unwrap_or("empty"));
}
```

This prints `first`. `enqueue` adds an item. `dequeue` removes the oldest item and returns `Option<T>`. `peek` reads it without removing it, also returning `Option<T>`. An empty queue returns `None`.

Use `.length`, `is_empty()`, and `clear()` to inspect or reset it. See [all Queue signatures](../../api/collections-queue.md). Use [PriorityQueue](priority-queue.md) when priority should decide which item comes next.

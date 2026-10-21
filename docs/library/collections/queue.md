---
title: Queue
summary: Process items in the order they arrive.
icon: material/tray-full
api: reference/api/system-collections.md
---

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

| Member | Returns | Effect |
| --- | --- | --- |
| `Queue<T>()` | `Queue<T>` | Empty queue. |
| `enqueue(value)` | `void` | Adds an item at the back. |
| `dequeue()` | `Option<T>` | Removes and returns the front item. |
| `peek()` | `Option<T>` | Front item, left in the queue. |
| `clear()` | `void` | Removes every item. |
| `.length` | `int` | Number of items. |
| `.capacity` | `int` | Items that fit before the queue must grow. |
| `is_empty()` | `bool` | `true` when the queue has no items. |
| `reserve(additional)` | `Result<Unit, CapacityError>` | Makes room for `additional` more items; `Err` for a negative or unsupported count. |

A `for` loop visits items from front to back without removing them. Queues also have the [query helpers](seq.md) such as `filter` and `any`.

See [all Queue signatures](../../reference/api/collections-queue.md). Use [PriorityQueue](priority-queue.md) when priority should decide which item comes next.

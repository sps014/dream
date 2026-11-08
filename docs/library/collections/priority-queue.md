---
title: PriorityQueue
summary: Remove the smallest or highest-priority item first.
icon: material/sort-numeric-ascending
api: reference/api/system-collections.md
---

# PriorityQueue: choose the next item by priority

A priority queue removes the smallest item first by default. Use it when importance or a scheduled order matters more than arrival time.

**Import:** `import system.collections;` · **Availability:** desktop, browser, and Node.

```dream
import system;
import system.collections;

fun main() {
    let jobs = PriorityQueue<int>();
    jobs.push(30);
    jobs.push(10);
    jobs.push(20);
    System.println(jobs.pop().unwrap_or(-1));
    System.println(jobs.peek().unwrap_or(-1));
}
```

The example prints `10`, then `20`. `peek()` leaves the item in the queue, `pop()` removes it, and both return `None` when empty.

## Choose a different order

The no-argument constructor uses the element's own order. Pass a comparison to choose another: negative if the first value should come out first, zero if equal, positive if the second should come out first.

```dream
let largest_first = PriorityQueue<int>((a, b) => b.compare(a));
```

This is a setup snippet for use inside a function. Prefer `compare` over subtraction, which can overflow for distant integer values.

## Members

| Member | Returns | Effect |
| --- | --- | --- |
| `PriorityQueue<T>()` | `PriorityQueue<T>` | Empty queue, smallest first; requires `T: Comparable<T>`. |
| `PriorityQueue<T>(compare)` | `PriorityQueue<T>` | Empty queue ordered by your comparison function. |
| `push(value)` | `void` | Adds an item. |
| `pop()` | `Option<T>` | Removes and returns the item that comes out first. |
| `peek()` | `Option<T>` | Item that would come out next, left in the queue. |
| `clear()` | `void` | Removes every item. |
| `.length` | `int` | Number of items. |
| `is_empty()` | `bool` | `true` when the queue has no items. |

A `for` loop walks a sorted copy from when the loop starts, and later pushes or pops do not change that copy. Helpers such as `filter` are in [queries](seq.md); signatures are in the [PriorityQueue reference](../../reference/api/collections-priority-queue.md).

Equal-priority elements have no guaranteed order. A comparator may read the queue, but `push`, `pop`, or `clear` on that same queue from the comparator stops the program.

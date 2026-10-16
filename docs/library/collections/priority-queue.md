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

Output: `10`, then `20` on a new line. `peek()` leaves the item in the queue. `pop()` removes it. Both return `None` when empty.

## Choose a different order

The no-argument constructor uses the element's natural comparison order. Supply a comparison function to choose another order. It must return a negative number when the first value should come out first, zero when equal, and a positive number when the second should come out first.

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

A `for` loop visits a sorted copy of the items taken when the loop starts, in priority order. The loop does not remove anything, and pushing or popping during the loop does not change what it visits. Priority queues also have the [query helpers](seq.md) such as `filter` and `any`. See [all PriorityQueue signatures](../../reference/api/collections-priority-queue.md).

The queue uses a four-child heap. Equal-priority elements have no guaranteed order.
A comparator may read the queue and update unrelated state, but calling `push`, `pop`, or `clear` on the same queue from its comparator panics before mutation.

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

Read `.length`, use `is_empty()`, or call `clear()` to reset the queue. Iteration shows its stored items; repeated `pop()` calls produce priority order. See [all PriorityQueue signatures](../../api/collections-priority-queue.md).

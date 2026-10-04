# Debug

`Debug` exposes allocator diagnostics from `system`.

| Member | Type | Meaning |
|---|---|---|
| `live_objects` | `long` | Current live heap allocations, excluding pinned immortal singletons |
| `total_allocations` | `long` | Cumulative allocations, including recycled blocks |
| `ref_count(borrow value)` | `int` | Strong reference count of a managed object |
| `heap_ptr` | `int` | Allocator position probe; its interpretation depends on the target |
| `free_list_head` | `int` | Free-activity probe, not a portable heap address |

Allocation counters are 64-bit and atomically updated on native and wasm32. Native totals
combine per-thread tallies that remain registered after a worker exits. Each native tally
has one writer: its owning thread. Atomic loads/stores allow diagnostic readers without
locked read-modify-write instructions on allocation/free paths; shared wasm32 counters
use atomic read-modify-write updates. Relaxed reads may
observe different threads at different instants; join workers before asserting a final
balance. These counters do not order or synchronize application data.

```dream
let before: long = Debug.live_objects;
churn();
let delta: long = Debug.live_objects - before;
System.println(delta);
```

Native debug executables print a leak report at exit. Release executables opt in with
`DREAM_DEBUG_LEAKS=1`. Embedded libraries leave reporting to their host.

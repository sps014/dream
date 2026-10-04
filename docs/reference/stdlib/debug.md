# Check memory use while developing

`Debug` helps you inspect memory use. Import `system` to use it. These values are development aids; they do not coordinate work running in different tasks.

| Member | Type | Meaning |
| --- | --- | --- |
| `live_objects` | `long` | Number of live allocated objects, excluding permanent shared values |
| `total_allocations` | `long` | Total allocation count, including reused storage |
| `ref_count(value)` | `int` | Number of strong references to a managed value |
| `heap_ptr` | `int` | A memory-position probe; its meaning depends on the environment |
| `free_list_head` | `int` | A memory-reuse probe, not a portable address |

## Compare before and after

The following is a snippet for a function that already defines `churn()`:

```dream
let before: long = Debug.live_objects;
churn();
let delta: long = Debug.live_objects - before;
System.println(delta);
```

Finish worker tasks before comparing a final count. During concurrent work, different counters may reflect different instants. A changed count alone does not prove a leak: values may still be alive intentionally.

Desktop debug programs print a leak report at exit. Release programs can enable it with `DREAM_DEBUG_LEAKS=1`. Embedded libraries leave reporting to their host application.

See [Debug declarations](../api/debug.md) and [Memory management](../language/memory.md).

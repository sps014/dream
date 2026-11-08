---
title: Debug
summary: Inspect memory use while developing.
icon: material/bug-outline
---

# Check memory use while developing

Import `system` and use `Debug` to inspect memory while developing. These values do not coordinate work across tasks.

Only a debug build counts allocations. In `--release`, `live_objects`, `total_allocations`, and `free_list_head` stay `0`, so run memory checks in a debug build.

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

Finish worker tasks before comparing a final count. During concurrent work the counters can disagree, and a higher count can be objects you still mean to keep.

Desktop debug programs print a leak report to stderr at exit:

```text
[dream] leak check: live=2 total_allocations=41 live_bytes=96
[dream] leak by type: Node=2 (96 B)
```

`live` is objects still allocated, and `live_bytes` is the memory they use. The next line lists the 16 largest types, then `other=` when more leaked; a release build prints nothing, and an embedded library leaves the report to its host.

See [Debug declarations](../../reference/api/debug.md) and [Memory management](../../language/memory/index.md).
